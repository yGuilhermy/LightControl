use crate::{crypto, model::Dps};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HEADER: usize = 18;
const MAX_FRAME: usize = 64 * 1024;
pub fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct Frame {
    pub command: u32,
    pub payload: Vec<u8>,
}
fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

pub fn encode(
    seq: u32,
    command: u32,
    payload: &[u8],
    key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, String> {
    let mut frame = Vec::with_capacity(payload.len() + 50);
    frame.extend_from_slice(&0x6699u32.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&((payload.len() + 28) as u32).to_be_bytes());
    let (cipher, tag) = crypto::gcm(key, iv, &frame[4..], payload, None)?;
    frame.extend_from_slice(iv);
    frame.extend_from_slice(&cipher);
    frame.extend_from_slice(&tag);
    frame.extend_from_slice(&0x9966u32.to_be_bytes());
    Ok(frame)
}

pub fn decode(bytes: &[u8], key: &[u8]) -> Result<Frame, String> {
    if bytes.len() < 50 || bytes[..4] != [0, 0, 0x66, 0x99] {
        return Err("Cabeçalho Tuya inválido.".into());
    }
    let len = u32be(&bytes[14..18]) as usize;
    if !(28..=MAX_FRAME).contains(&len)
        || len + HEADER + 4 != bytes.len()
        || bytes[bytes.len() - 4..] != [0, 0, 0x99, 0x66]
    {
        return Err("Tamanho ou fim de mensagem Tuya inválido.".into());
    }
    let (payload, _) = crypto::gcm(
        key,
        &bytes[18..30],
        &bytes[4..18],
        &bytes[30..18 + len - 16],
        Some(&bytes[18 + len - 16..18 + len]),
    )?;
    Ok(Frame {
        command: u32be(&bytes[10..14]),
        payload,
    })
}

fn trim_payload(mut p: &[u8]) -> &[u8] {
    if p.len() >= 4 && p[..4] == [0, 0, 0, 0] {
        p = &p[4..];
    }
    if p.starts_with(b"3.5") && p.len() >= 15 {
        p = &p[15..];
    }
    while p.last() == Some(&0) {
        p = &p[..p.len() - 1];
    }
    p
}
fn extract_dps(frame: &Frame) -> Option<Dps> {
    let v: Value = serde_json::from_slice(trim_payload(&frame.payload)).ok()?;
    v.get("dps")
        .or_else(|| v.get("data").and_then(|d| d.get("dps")))
        .and_then(Value::as_object)
        .cloned()
}

pub struct Client {
    stream: TcpStream,
    key: Vec<u8>,
    seq: u32,
    buffer: Vec<u8>,
}
impl Drop for Client {
    fn drop(&mut self) {
        crypto::wipe(&mut self.key);
    }
}
impl Client {
    pub fn connect(ip: &str, key: &str) -> Result<Self, String> {
        let ip = ip
            .parse::<Ipv4Addr>()
            .map_err(|_| "Configure um IPv4 válido.")?;
        Self::connect_addr(SocketAddr::from((ip, 6668)), key)
    }
    fn connect_addr(address: SocketAddr, key: &str) -> Result<Self, String> {
        if key.len() != 16 {
            return Err("A chave local precisa ter 16 bytes.".into());
        }
        let stream = TcpStream::connect_timeout(&address, Duration::from_millis(1200))
            .map_err(|_| "Lâmpada indisponível na rede local.")?;
        stream.set_nodelay(true).ok();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
        let mut c = Self {
            stream,
            key: key.as_bytes().to_vec(),
            seq: 1,
            buffer: Vec::new(),
        };
        let mut nonce = crypto::random::<16>()?;
        c.send(3, &nonce)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let response = loop {
            let r = c.receive(deadline)?;
            if r.command == 4 {
                break r;
            }
        };
        let p = if response.payload.len() == 52 && response.payload[..4] == [0, 0, 0, 0] {
            &response.payload[4..]
        } else {
            &response.payload[..]
        };
        if p.len() != 48 || !crypto::constant_eq(&crypto::hmac(key.as_bytes(), &nonce)?, &p[16..]) {
            return Err("A lâmpada recusou a chave local. Reimporte a credencial ou feche outro controlador.".into());
        }
        let remote = &p[..16];
        c.send(5, &crypto::hmac(key.as_bytes(), remote)?)?;
        let mut mixed = [0; 16];
        for i in 0..16 {
            mixed[i] = nonce[i] ^ remote[i];
        }
        let (session, _) = crypto::gcm(key.as_bytes(), &nonce[..12], &[], &mixed, None)?;
        crypto::wipe(&mut c.key);
        c.key = session;
        crypto::wipe(&mut mixed);
        crypto::wipe(&mut nonce);
        Ok(c)
    }
    fn send(&mut self, command: u32, payload: &[u8]) -> Result<(), String> {
        let frame = encode(
            self.seq,
            command,
            payload,
            &self.key,
            &crypto::random::<12>()?,
        )?;
        self.seq = self.seq.checked_add(1).ok_or("Renove a conexão Tuya.")?;
        self.stream
            .write_all(&frame)
            .map_err(|_| "Falha ao enviar comando local.".into())
    }
    fn receive(&mut self, deadline: Instant) -> Result<Frame, String> {
        loop {
            if self.buffer.len() >= HEADER {
                if self.buffer[..4] != [0, 0, 0x66, 0x99] {
                    return Err("Resposta com protocolo incompatível.".into());
                }
                let len = u32be(&self.buffer[14..18]) as usize;
                if !(28..=MAX_FRAME).contains(&len) {
                    return Err("Resposta Tuya excedeu o limite.".into());
                }
                let total = HEADER + len + 4;
                if self.buffer.len() >= total {
                    let frame = decode(&self.buffer[..total], &self.key)?;
                    self.buffer.drain(..total);
                    return Ok(frame);
                }
            }
            if Instant::now() >= deadline {
                return Err("A lâmpada não confirmou a operação a tempo.".into());
            }
            let mut bytes = [0u8; 4096];
            match self.stream.read(&mut bytes) {
                Ok(0) => return Err("A lâmpada encerrou a conexão.".into()),
                Ok(n) => {
                    self.buffer.extend_from_slice(&bytes[..n]);
                    if self.buffer.len() > MAX_FRAME + HEADER + 4 {
                        return Err("Resposta local muito grande.".into());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return Err("Conexão local interrompida.".into()),
            }
        }
    }
    pub fn query(&mut self) -> Result<Dps, String> {
        self.send(16, b"{}")?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let r = self.receive(deadline)?;
            if let Some(d) = extract_dps(&r) {
                return Ok(d);
            }
        }
    }
    pub fn set(&mut self, dps: &Dps) -> Result<Dps, String> {
        let mut p = b"3.5\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        p.extend_from_slice(
            &serde_json::to_vec(&json!({"protocol":5,"t":epoch(),"data":{"dps":dps}})).unwrap(),
        );
        self.send(13, &p)?;
        self.send(16, b"{}")?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut observed = Dps::new();
        loop {
            let frame = self.receive(deadline)?;
            if let Some(d) = extract_dps(&frame) {
                observed.extend(d);
                if dps.iter().all(|(k, v)| {
                    observed.get(k).is_some_and(|actual| {
                        actual == v
                            || (k == "26"
                                && actual
                                    .as_u64()
                                    .zip(v.as_u64())
                                    .is_some_and(|(a, b)| a <= b && b - a <= 3))
                    })
                }) {
                    return Ok(observed);
                }
            }
        }
    }
    pub fn heartbeat(&mut self) -> Result<(), String> {
        self.send(9, b"{}")
    }
    pub fn poll(&mut self) -> Result<Option<Dps>, String> {
        let mut peek = [0; 1];
        self.stream.set_nonblocking(true).ok();
        let available = self.stream.peek(&mut peek);
        self.stream.set_nonblocking(false).ok();
        match available {
            Ok(0) => Err("A lâmpada desconectou.".into()),
            Ok(_) => {
                let f = self.receive(Instant::now() + Duration::from_secs(2))?;
                Ok(extract_dps(&f))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err("Conexão local interrompida.".into()),
        }
    }
}

pub fn discover(id: &str, timeout: Duration) -> Result<String, String> {
    let sockets: Vec<UdpSocket> = [6666, 6667, 7000]
        .into_iter()
        .filter_map(|p| UdpSocket::bind((Ipv4Addr::UNSPECIFIED, p)).ok())
        .collect();
    if sockets.is_empty() {
        return Err("Portas de descoberta em uso; informe o IP manualmente.".into());
    }
    for s in &sockets {
        s.set_nonblocking(true).ok();
    }
    let key = [
        108, 30, 200, 226, 187, 155, 181, 154, 181, 11, 13, 175, 100, 155, 65, 10,
    ];
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        for s in &sockets {
            let mut buffer = [0; 4096];
            if let Ok((n, addr)) = s.recv_from(&mut buffer) {
                let b = &buffer[..n];
                let payload = if b.starts_with(&[0, 0, 0x66, 0x99]) {
                    decode(b, &key).map(|f| trim_payload(&f.payload).to_vec())
                } else if b.starts_with(&[0, 0, 0x55, 0xaa]) && n > 28 {
                    let p = &b[20..n - 8];
                    if p.starts_with(b"{") {
                        Ok(p.to_vec())
                    } else {
                        crypto::ecb_decrypt(&key, p)
                    }
                } else {
                    crypto::ecb_decrypt(&key, b)
                };
                if let Ok(p) = payload {
                    if let Ok(v) = serde_json::from_slice::<Value>(&p) {
                        if v.get("gwId")
                            .or_else(|| v.get("devId"))
                            .and_then(Value::as_str)
                            == Some(id)
                        {
                            return Ok(addr.ip().to_string());
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Err("Nenhum anúncio da lâmpada foi recebido. Verifique o IP e a rede.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_authenticates_header_and_payload() {
        let key = b"0123456789abcdef";
        let mut f = encode(7, 16, b"{}", key, &[1; 12]).unwrap();
        let r = decode(&f, key).unwrap();
        assert_eq!(r.command, 16);
        assert_eq!(r.payload, b"{}");
        f[10] ^= 1;
        assert!(decode(&f, key).is_err());
    }
    #[test]
    fn corrupted_lengths_do_not_panic() {
        for len in [0u32, 1, 27, 65537, u32::MAX] {
            let mut f = vec![0; 50];
            f[..4].copy_from_slice(&[0, 0, 0x66, 0x99]);
            f[14..18].copy_from_slice(&len.to_be_bytes());
            assert!(decode(&f, b"0123456789abcdef").is_err());
        }
    }
    #[test]
    fn partial_status_and_version_header() {
        let f = Frame {
            command: 8,
            payload: [
                vec![0; 4],
                b"3.5\0\0\0\0\0\0\0\0\0\0\0\0".to_vec(),
                br#"{"data":{"dps":{"20":false}}}"#.to_vec(),
            ]
            .concat(),
        };
        assert_eq!(extract_dps(&f).unwrap()["20"], false);
    }
    #[test]
    fn interoperates_with_tinytuya_frame() {
        let frame = decode(
            include_bytes!("../tests/fixtures/status35.bin"),
            b"0123456789abcdef",
        )
        .unwrap();
        assert_eq!(frame.command, 8);
        let d = extract_dps(&frame).unwrap();
        assert_eq!(d["20"], false);
        assert_eq!(d["22"], 505);
        assert_eq!(d["23"], 400);
    }
    #[test]
    fn tcp_fragmentation_coalescing_and_readback() {
        use std::net::TcpListener;
        fn read(s: &mut TcpStream, key: &[u8]) -> Frame {
            let mut header = [0; 18];
            s.read_exact(&mut header).unwrap();
            let len = u32be(&header[14..18]) as usize;
            let mut bytes = header.to_vec();
            bytes.resize(18 + len + 4, 0);
            s.read_exact(&mut bytes[18..]).unwrap();
            decode(&bytes, key).unwrap()
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
            let key = b"0123456789abcdef";
            let hello = read(&mut s, key);
            assert_eq!(hello.command, 3);
            assert_eq!(hello.payload.len(), 16);
            let remote = [0x79; 16];
            let response = [
                vec![0; 4],
                remote.to_vec(),
                crypto::hmac(key, &hello.payload).unwrap().to_vec(),
            ]
            .concat();
            let frame = encode(1, 4, &response, key, &[3; 12]).unwrap();
            for chunk in frame.chunks(3) {
                s.write_all(chunk).unwrap();
            }
            let finish = read(&mut s, key);
            assert_eq!(finish.command, 5);
            assert_eq!(finish.payload, crypto::hmac(key, &remote).unwrap());
            let mixed: Vec<u8> = hello
                .payload
                .iter()
                .zip(remote)
                .map(|(a, b)| a ^ b)
                .collect();
            let session = crypto::gcm(key, &hello.payload[..12], &[], &mixed, None)
                .unwrap()
                .0;
            let request = read(&mut s, &session);
            assert_eq!(request.command, 16);
            let initial = [vec![0; 4], br#"{"dps":{"20":false}}"#.to_vec()].concat();
            s.write_all(&encode(2, 8, &initial, &session, &[4; 12]).unwrap())
                .unwrap();
            let set = read(&mut s, &session);
            assert_eq!(set.command, 13);
            let value: Value = serde_json::from_slice(&set.payload[15..]).unwrap();
            assert_eq!(value["data"]["dps"]["20"], true);
            assert_eq!(read(&mut s, &session).command, 16);
            let confirmed = [vec![0; 4], br#"{"dps":{"20":true}}"#.to_vec()].concat();
            let combined = [
                encode(3, 13, &[0; 4], &session, &[5; 12]).unwrap(),
                encode(4, 8, &initial, &session, &[6; 12]).unwrap(),
                encode(5, 8, &confirmed, &session, &[7; 12]).unwrap(),
            ]
            .concat();
            s.write_all(&combined).unwrap();
        });
        let mut c = Client::connect_addr(address, "0123456789abcdef").unwrap();
        assert_eq!(c.query().unwrap()["20"], false);
        assert_eq!(
            c.set(json!({"20":true}).as_object().unwrap()).unwrap()["20"],
            true
        );
        server.join().unwrap();
    }
}
