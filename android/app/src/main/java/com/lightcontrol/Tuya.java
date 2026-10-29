package com.lightcontrol;

import java.io.*;
import java.net.*;
import java.nio.*;
import java.nio.charset.StandardCharsets;
import java.security.*;
import java.util.*;
import javax.crypto.*;
import javax.crypto.spec.*;
import org.json.JSONObject;

public final class Tuya implements AutoCloseable {
  static final int MAX_FRAME = 65536;
  static final SecureRandom RANDOM = new SecureRandom();
  final Socket socket;
  final DataInputStream input;
  final OutputStream output;
  byte[] key;
  int sequence = 1;

  public static final class Frame {
    final int command;
    final byte[] payload;

    Frame(int c, byte[] p) {
      command = c;
      payload = p;
    }
  }

  static byte[] random(int n) {
    byte[] b = new byte[n];
    RANDOM.nextBytes(b);
    return b;
  }

  static byte[] hmac(byte[] k, byte[] p) throws Exception {
    Mac m = Mac.getInstance("HmacSHA256");
    m.init(new SecretKeySpec(k, "HmacSHA256"));
    return m.doFinal(p);
  }

  static byte[] aes(byte[] k, byte[] iv, byte[] aad, byte[] p, int mode) throws Exception {
    Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
    c.init(mode, new SecretKeySpec(k, "AES"), new GCMParameterSpec(128, iv));
    c.updateAAD(aad);
    return c.doFinal(p);
  }

  static byte[] encode(int seq, int cmd, byte[] p, byte[] k, byte[] iv) throws Exception {
    ByteBuffer b = ByteBuffer.allocate(p.length + 50).order(ByteOrder.BIG_ENDIAN);
    b.putInt(0x6699).putShort((short) 0).putInt(seq).putInt(cmd).putInt(p.length + 28);
    byte[] aad = Arrays.copyOfRange(b.array(), 4, 18);
    b.put(iv).put(aes(k, iv, aad, p, Cipher.ENCRYPT_MODE)).putInt(0x9966);
    return b.array();
  }

  static Frame decode(byte[] b, byte[] k) throws Exception {
    if (b.length < 50) throw new IOException("Mensagem Tuya incompleta");
    ByteBuffer h = ByteBuffer.wrap(b).order(ByteOrder.BIG_ENDIAN);
    int len = h.getInt(14);
    if (h.getInt() != 0x6699
        || len < 28
        || len > MAX_FRAME
        || b.length != len + 22
        || h.getInt(b.length - 4) != 0x9966) throw new IOException("Mensagem Tuya inválida");
    return new Frame(
        h.getInt(10),
        aes(
            k,
            Arrays.copyOfRange(b, 18, 30),
            Arrays.copyOfRange(b, 4, 18),
            Arrays.copyOfRange(b, 30, b.length - 4),
            Cipher.DECRYPT_MODE));
  }

  static byte[] trim(byte[] b) {
    int start = 0, end = b.length;
    if (end >= 4 && b[0] == 0 && b[1] == 0 && b[2] == 0 && b[3] == 0) start = 4;
    if (end - start >= 15 && b[start] == '3' && b[start + 1] == '.' && b[start + 2] == '5')
      start += 15;
    while (end > start && b[end - 1] == 0) end--;
    return Arrays.copyOfRange(b, start, end);
  }

  static JSONObject dps(Frame f) {
    try {
      JSONObject j = new JSONObject(new String(trim(f.payload), StandardCharsets.UTF_8));
      JSONObject d = j.optJSONObject("dps");
      return d != null
          ? d
          : j.optJSONObject("data") != null ? j.getJSONObject("data").optJSONObject("dps") : null;
    } catch (Exception e) {
      return null;
    }
  }

  public Tuya(String ip, byte[] localKey) throws Exception {
    this(ip, 6668, localKey);
  }

  Tuya(String ip, int port, byte[] localKey) throws Exception {
    if (localKey.length != 16) throw new IOException("A chave local precisa ter 16 bytes");
    validateIp(ip);
    key = localKey.clone();
    socket = new Socket();
    try {
      socket.connect(new InetSocketAddress(ip, port), 1200);
      socket.setSoTimeout(3000);
      socket.setTcpNoDelay(true);
      input = new DataInputStream(new BufferedInputStream(socket.getInputStream()));
      output = socket.getOutputStream();
      byte[] nonce = random(16);
      send(3, nonce);
      Frame f;
      long end = System.nanoTime() + 3_000_000_000L;
      do {
        f = receive(end);
      } while (f.command != 4);
      byte[] p = f.payload;
      if (p.length == 52) p = Arrays.copyOfRange(p, 4, p.length);
      if (p.length != 48 || !MessageDigest.isEqual(hmac(key, nonce), Arrays.copyOfRange(p, 16, 48)))
        throw new IOException("Chave local recusada ou outro controlador conectado");
      byte[] remote = Arrays.copyOf(p, 16);
      send(5, hmac(key, remote));
      byte[] mixed = new byte[16];
      for (int i = 0; i < 16; i++) mixed[i] = (byte) (nonce[i] ^ remote[i]);
      byte[] session = aes(key, Arrays.copyOf(nonce, 12), new byte[0], mixed, Cipher.ENCRYPT_MODE);
      Arrays.fill(key, (byte) 0);
      key = Arrays.copyOf(session, 16);
      Arrays.fill(session, (byte) 0);
      Arrays.fill(mixed, (byte) 0);
      Arrays.fill(nonce, (byte) 0);
    } catch (Exception e) {
      socket.close();
      Arrays.fill(key, (byte) 0);
      throw e;
    }
  }

  static void validateIp(String ip) throws IOException {
    String[] a = ip.split("\\.", -1);
    if (a.length != 4) throw new IOException("Informe um IPv4 válido");
    try {
      for (String s : a)
        if (s.isEmpty() || !s.matches("[0-9]{1,3}") || Integer.parseInt(s) > 255)
          throw new NumberFormatException();
    } catch (NumberFormatException e) {
      throw new IOException("Informe um IPv4 válido");
    }
  }

  void send(int cmd, byte[] p) throws Exception {
    if (sequence == Integer.MAX_VALUE) throw new IOException("Renove a conexão");
    output.write(encode(sequence++, cmd, p, key, random(12)));
    output.flush();
  }

  Frame receive(long deadline) throws Exception {
    long ms = (deadline - System.nanoTime()) / 1_000_000;
    if (ms <= 0) throw new SocketTimeoutException("Sem confirmação da lâmpada");
    socket.setSoTimeout((int) Math.max(1, ms));
    byte[] header = new byte[18];
    input.readFully(header);
    int len = ByteBuffer.wrap(header).getInt(14);
    if (ByteBuffer.wrap(header).getInt() != 0x6699 || len < 28 || len > MAX_FRAME)
      throw new IOException("Cabeçalho Tuya inválido");
    byte[] packet = Arrays.copyOf(header, len + 22);
    input.readFully(packet, 18, len + 4);
    return decode(packet, key);
  }

  public JSONObject query() throws Exception {
    send(16, "{}".getBytes(StandardCharsets.UTF_8));
    long end = System.nanoTime() + 3_000_000_000L;
    while (true) {
      JSONObject d = dps(receive(end));
      if (d != null) return d;
    }
  }

  public JSONObject set(JSONObject desired) throws Exception {
    JSONObject body =
        new JSONObject()
            .put("protocol", 5)
            .put("t", System.currentTimeMillis() / 1000)
            .put("data", new JSONObject().put("dps", desired));
    byte[] json = body.toString().getBytes(StandardCharsets.UTF_8), p = new byte[15 + json.length];
    p[0] = '3';
    p[1] = '.';
    p[2] = '5';
    System.arraycopy(json, 0, p, 15, json.length);
    send(13, p);
    send(16, "{}".getBytes(StandardCharsets.UTF_8));
    JSONObject observed = new JSONObject();
    long end = System.nanoTime() + 3_000_000_000L;
    while (true) {
      JSONObject d = dps(receive(end));
      if (d == null) continue;
      merge(observed, d);
      boolean done = true;
      for (Iterator<String> it = desired.keys(); it.hasNext(); ) {
        String k = it.next();
        Object actual = observed.opt(k), wanted = desired.get(k);
        boolean eq =
            Objects.equals(actual, wanted)
                || actual instanceof Number
                    && wanted instanceof Number
                    && ((Number) actual).doubleValue() == ((Number) wanted).doubleValue();
        if (k.equals("26") && actual instanceof Number && wanted instanceof Number) {
          long delta = ((Number) wanted).longValue() - ((Number) actual).longValue();
          eq = delta >= 0 && delta <= 3;
        }
        if (!eq) done = false;
      }
      if (done) return observed;
    }
  }

  public JSONObject poll() throws Exception {
    if (input.available() == 0) return null;
    return dps(receive(System.nanoTime() + 2_000_000_000L));
  }

  public void heartbeat() throws Exception {
    send(9, "{}".getBytes(StandardCharsets.UTF_8));
  }

  static void merge(JSONObject target, JSONObject from) throws Exception {
    for (Iterator<String> i = from.keys(); i.hasNext(); ) {
      String k = i.next();
      target.put(k, from.get(k));
    }
  }

  public void close() {
    try {
      socket.close();
    } catch (Exception ignored) {
    }
    Arrays.fill(key, (byte) 0);
  }

  public static String discover(String id) throws Exception {
    List<DatagramSocket> sockets = new ArrayList<>();
    byte[] dk = {
      108,
      30,
      (byte) 200,
      (byte) 226,
      (byte) 187,
      (byte) 155,
      (byte) 181,
      (byte) 154,
      (byte) 181,
      11,
      13,
      (byte) 175,
      100,
      (byte) 155,
      65,
      10
    };
    try {
      for (int port : new int[] {6666, 6667, 7000}) {
        try {
          DatagramSocket s = new DatagramSocket(null);
          s.setReuseAddress(true);
          s.bind(new InetSocketAddress(port));
          s.setSoTimeout(40);
          sockets.add(s);
        } catch (SocketException ignored) {
        }
      }
      if (sockets.isEmpty())
        throw new IOException("Portas de descoberta indisponíveis. Informe o IP.");
      long end = System.nanoTime() + 5_000_000_000L;
      while (System.nanoTime() < end)
        for (DatagramSocket s : sockets) {
          byte[] buf = new byte[4096];
          DatagramPacket p = new DatagramPacket(buf, buf.length);
          try {
            s.receive(p);
            byte[] b = Arrays.copyOf(buf, p.getLength()), plain;
            if (b.length >= 4 && ByteBuffer.wrap(b).getInt() == 0x6699)
              plain = trim(decode(b, dk).payload);
            else {
              if (b.length > 28 && ByteBuffer.wrap(b).getInt() == 0x55aa)
                b = Arrays.copyOfRange(b, 20, b.length - 8);
              if (b.length > 0 && b[0] == '{') plain = b;
              else {
                Cipher c = Cipher.getInstance("AES/ECB/PKCS5Padding");
                c.init(Cipher.DECRYPT_MODE, new SecretKeySpec(dk, "AES"));
                plain = c.doFinal(b);
              }
            }
            JSONObject j = new JSONObject(new String(plain, StandardCharsets.UTF_8));
            if (id.equals(j.optString("gwId", j.optString("devId"))))
              return p.getAddress().getHostAddress();
          } catch (SocketTimeoutException ignored) {
          } catch (GeneralSecurityException | org.json.JSONException ignored) {
          }
        }
      throw new IOException("Nenhum anúncio recebido. Verifique o Wi-Fi e o IP.");
    } finally {
      for (DatagramSocket s : sockets) s.close();
    }
  }
}
