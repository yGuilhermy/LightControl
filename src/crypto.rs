use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};

fn check(status: i32) -> Result<(), String> {
    if status >= 0 {
        Ok(())
    } else {
        Err(format!("Falha criptográfica do Windows ({status:#x})."))
    }
}
struct Algorithm(BCRYPT_ALG_HANDLE);
impl Drop for Algorithm {
    fn drop(&mut self) {
        unsafe {
            BCryptCloseAlgorithmProvider(self.0, 0);
        }
    }
}
struct Key(BCRYPT_KEY_HANDLE);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.0);
        }
    }
}
fn algorithm(name: *const u16, flags: u32) -> Result<Algorithm, String> {
    unsafe {
        let mut h = null_mut();
        check(BCryptOpenAlgorithmProvider(&mut h, name, null(), flags))?;
        Ok(Algorithm(h))
    }
}
pub fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut out = [0; N];
    unsafe {
        check(BCryptGenRandom(
            null_mut(),
            out.as_mut_ptr(),
            N as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        ))?;
    }
    Ok(out)
}
pub fn hmac(key: &[u8], data: &[u8]) -> Result<[u8; 32], String> {
    let a = algorithm(BCRYPT_SHA256_ALGORITHM, BCRYPT_ALG_HANDLE_HMAC_FLAG)?;
    let mut out = [0; 32];
    unsafe {
        check(BCryptHash(
            a.0,
            key.as_ptr(),
            key.len() as u32,
            data.as_ptr(),
            data.len() as u32,
            out.as_mut_ptr(),
            32,
        ))?;
    }
    Ok(out)
}
pub fn ecb_decrypt(key: &[u8], input: &[u8]) -> Result<Vec<u8>, String> {
    let a = algorithm(BCRYPT_AES_ALGORITHM, 0)?;
    let mode: Vec<u16> = "ChainingModeECB\0".encode_utf16().collect();
    let mut h = null_mut();
    unsafe {
        check(BCryptSetProperty(
            a.0,
            BCRYPT_CHAINING_MODE,
            mode.as_ptr() as _,
            (mode.len() * 2) as u32,
            0,
        ))?;
        check(BCryptGenerateSymmetricKey(
            a.0,
            &mut h,
            null_mut(),
            0,
            key.as_ptr(),
            key.len() as u32,
            0,
        ))?;
    }
    let h = Key(h);
    let mut out = vec![0; input.len()];
    let mut len = 0;
    unsafe {
        check(BCryptDecrypt(
            h.0,
            input.as_ptr(),
            input.len() as u32,
            null(),
            null_mut(),
            0,
            out.as_mut_ptr(),
            out.len() as u32,
            &mut len,
            BCRYPT_BLOCK_PADDING,
        ))?;
    }
    out.truncate(len as usize);
    Ok(out)
}

pub fn gcm(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    input: &[u8],
    tag: Option<&[u8]>,
) -> Result<(Vec<u8>, [u8; 16]), String> {
    if key.len() != 16 || iv.len() != 12 || tag.is_some_and(|v| v.len() != 16) {
        return Err("Paramètres AES invalides.".into());
    }
    let a = algorithm(BCRYPT_AES_ALGORITHM, 0)?;
    let mode: Vec<u16> = "ChainingModeGCM\0".encode_utf16().collect();
    let mut key_handle = null_mut();
    unsafe {
        check(BCryptSetProperty(
            a.0,
            BCRYPT_CHAINING_MODE,
            mode.as_ptr() as *const u8,
            (mode.len() * 2) as u32,
            0,
        ))?;
        check(BCryptGenerateSymmetricKey(
            a.0,
            &mut key_handle,
            null_mut(),
            0,
            key.as_ptr(),
            key.len() as u32,
            0,
        ))?;
    }
    let key_handle = Key(key_handle);
    let mut out = vec![0; input.len().max(1)];
    let mut out_len = 0;
    let mut iv = iv.to_vec();
    let mut aad = aad.to_vec();
    let mut auth_tag = [0; 16];
    if let Some(t) = tag {
        auth_tag.copy_from_slice(t);
    }
    let mut info: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO = unsafe { zeroed() };
    info.cbSize = size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32;
    info.dwInfoVersion = 1;
    info.pbNonce = iv.as_mut_ptr();
    info.cbNonce = iv.len() as u32;
    info.pbAuthData = aad.as_mut_ptr();
    info.cbAuthData = aad.len() as u32;
    info.pbTag = auth_tag.as_mut_ptr();
    info.cbTag = 16;
    unsafe {
        let op = if tag.is_some() {
            BCryptDecrypt
        } else {
            BCryptEncrypt
        };
        check(op(
            key_handle.0,
            input.as_ptr(),
            input.len() as u32,
            &mut info as *mut _ as *mut _,
            null_mut(),
            0,
            out.as_mut_ptr(),
            out.len() as u32,
            &mut out_len,
            0,
        ))?;
    }
    out.truncate(out_len as usize);
    Ok((out, auth_tag))
}

pub fn protect(data: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut _,
    };
    let mut out: CRYPT_INTEGER_BLOB = unsafe { zeroed() };
    let ok = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        }
    };
    if ok == 0 {
        return Err(
            "Não foi possível acessar a credencial protegida deste usuário do Windows.".into(),
        );
    }
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        LocalFree(out.pbData as _);
    }
    Ok(bytes)
}

pub fn wipe(data: &mut [u8]) {
    for b in data {
        unsafe {
            std::ptr::write_volatile(b, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |v, (a, b)| v | (a ^ b)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nist_aes_gcm_and_tampering() {
        let (cipher, tag) = gcm(&[0; 16], &[0; 12], &[], &[0; 16], None).unwrap();
        assert_eq!(
            cipher,
            vec![
                0x03, 0x88, 0xda, 0xce, 0x60, 0xb6, 0xa3, 0x92, 0xf3, 0x28, 0xc2, 0xb9, 0x71, 0xb2,
                0xfe, 0x78
            ]
        );
        assert_eq!(
            tag,
            [
                0xab, 0x6e, 0x47, 0xd4, 0x2c, 0xec, 0x13, 0xbd, 0xf5, 0x3a, 0x67, 0xb2, 0x12, 0x57,
                0xbd, 0xdf
            ]
        );
        assert_eq!(
            gcm(&[0; 16], &[0; 12], &[], &cipher, Some(&tag)).unwrap().0,
            vec![0; 16]
        );
        let mut bad = tag;
        bad[0] ^= 1;
        assert!(gcm(&[0; 16], &[0; 12], &[], &cipher, Some(&bad)).is_err());
    }
    #[test]
    fn rfc4231_hmac() {
        let h = hmac(&[0x0b; 20], b"Hi There").unwrap();
        assert_eq!(
            h,
            [
                0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
                0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
                0x2e, 0x32, 0xcf, 0xf7
            ]
        );
    }
    #[test]
    fn credentials_roundtrip() {
        let p = protect(b"private test value", false).unwrap();
        assert_ne!(p, b"private test value");
        assert_eq!(protect(&p, true).unwrap(), b"private test value");
    }
}
