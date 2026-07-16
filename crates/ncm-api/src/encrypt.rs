use aes::Aes128;
use aes::cipher::{BlockCipherEncrypt, KeyInit};
use base64::{Engine, engine::general_purpose};
use md5::{Digest, Md5};
use rsa::RsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::BigUint;
use std::sync::LazyLock;

/// Raw RSA public key modulus and exponent for encryption without padding
struct RawRsaKey {
    n: BigUint,
    e: BigUint,
}

impl RawRsaKey {
    fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        let m = BigUint::from_bytes_be(data);
        let c = m.modpow(&self.e, &self.n);
        let bytes = c.to_bytes_be();
        // Pad to key size (128 bytes for 1024-bit key)
        let key_size = 128;
        if bytes.len() < key_size {
            let mut padded = vec![0u8; key_size - bytes.len()];
            padded.extend_from_slice(&bytes);
            padded
        } else {
            bytes
        }
    }
}

const IV: &[u8] = b"0102030405060708";
const PRESET_KEY: &[u8] = b"0CoJUm6Qyw8W8jud";
const EAPIKEY: &[u8] = b"e82ckenh8dichen8";
const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

// RSA public key PEM (without headers)
const RSA_PEM_BODY: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDgtQn2JZ34ZC28NWYpAUd98iZ37BUrX/aKzmFbt7clFSs6sXqHauqKWqdtLkF2KexO40H1YTX8z2lSgBBOAxLsvaklV8k4cBFK9snQXE9/DDaFt6Rr7iVZMldczhC0JNgTz+SHXT6CBHuX3e9SdB1Ua44oncaTWz7OBGLbCiK45wIDAQAB";

static RSA_KEY: LazyLock<RawRsaKey> = LazyLock::new(|| {
    use base64::Engine;
    let der = base64::engine::general_purpose::STANDARD
        .decode(RSA_PEM_BODY.replace('\n', "").trim())
        .expect("invalid base64 in RSA key");
    let pk = RsaPublicKey::from_public_key_der(&der).expect("invalid RSA public key DER");
    RawRsaKey {
        n: pk.n().clone(),
        e: pk.e().clone(),
    }
});

/// Web API 加密: 双重 AES-128-CBC + RSA
pub fn weapi(plaintext: &str) -> String {
    let secret_key = random_key_16();
    let key: Vec<u8> = secret_key
        .iter()
        .map(|b| BASE62[(*b % 62) as usize])
        .collect();

    let params1 = aes_cbc_encrypt(plaintext.as_bytes(), PRESET_KEY, IV);
    let params1_b64 = general_purpose::STANDARD.encode(&params1);

    let params2 = aes_cbc_encrypt(params1_b64.as_bytes(), &key, IV);
    let params = general_purpose::STANDARD.encode(&params2);

    let reversed_key: Vec<u8> = key.iter().rev().copied().collect();
    let enc_sec_key = rsa_encrypt(&reversed_key);

    format!(
        "params={}&encSecKey={}",
        urlencoding::encode(&params),
        urlencoding::encode(&enc_sec_key)
    )
}

/// E-API 加密: AES-128-ECB + MD5
pub fn eapi(url: &str, text: &str) -> String {
    let message = format!("nobody{}use{}md5forencrypt", url, text);
    let digest = md5_hex(&message);
    let data = format!("{}-36cd479b6b5-{}-36cd479b6b5-{}", url, text, digest);

    let encrypted = aes_ecb_encrypt(data.as_bytes(), EAPIKEY);
    let params = hex::encode_upper(&encrypted);

    format!("params={}", urlencoding::encode(&params))
}

/// PKCS7 padding
fn pkcs7_pad(data: &[u8], block_size: usize) -> Vec<u8> {
    let pad_len = block_size - (data.len() % block_size);
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat_n(pad_len as u8, pad_len));
    padded
}

/// AES-128-CBC 加密
fn aes_cbc_encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).unwrap();
    let padded = pkcs7_pad(data, 16);
    let mut result = vec![0u8; padded.len()];
    let mut prev_block = iv;

    for (i, chunk) in padded.chunks(16).enumerate() {
        // XOR with previous ciphertext block (or IV)
        let mut block = [0u8; 16];
        for j in 0..16 {
            block[j] = chunk[j] ^ prev_block[j];
        }
        // Encrypt block
        cipher.encrypt_block((&mut block).into());
        // Store result
        result[i * 16..(i + 1) * 16].copy_from_slice(&block);
        prev_block = &result[i * 16..(i + 1) * 16];
    }
    result
}

/// AES-128-ECB 加密
fn aes_ecb_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).unwrap();
    let padded = pkcs7_pad(data, 16);
    let mut result = padded.clone();

    for chunk in result.chunks_mut(16) {
        cipher.encrypt_block(chunk.try_into().unwrap());
    }
    result
}

/// RSA 加密 (NoPadding, textbook RSA)
fn rsa_encrypt(data: &[u8]) -> String {
    // Zero-pad to 128 bytes (RSA modulus size)
    let mut padded = vec![0u8; 128 - data.len()];
    padded.extend_from_slice(data);

    let encrypted = RSA_KEY.encrypt(&padded);
    hex::encode(&encrypted)
}

fn md5_hex(data: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

fn random_key_16() -> Vec<u8> {
    use rand::Rng;
    let mut buf = [0u8; 16];
    rand::rng().fill_bytes(&mut buf);
    buf.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5() {
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_weapi_produces_params_and_enc_sec_key() {
        let result = weapi(r#"{"username":"test"}"#);
        assert!(result.contains("params="));
        assert!(result.contains("encSecKey="));
    }

    #[test]
    fn test_eapi_produces_params() {
        let result = eapi("/api/song/enhance/player/url", r#"{"id":"123"}"#);
        assert!(result.contains("params="));
    }

    #[test]
    fn test_eapi_different_inputs_different_outputs() {
        let r1 = eapi("/api/song/enhance/player/url", r#"{"id":"123"}"#);
        let r2 = eapi("/api/song/enhance/player/url", r#"{"id":"456"}"#);
        assert_ne!(r1, r2, "不同输入应产生不同输出");
    }

    #[test]
    fn test_eapi_same_input_same_output() {
        let r1 = eapi("/api/song/enhance/player/url", r#"{"id":"123"}"#);
        let r2 = eapi("/api/song/enhance/player/url", r#"{"id":"123"}"#);
        assert_eq!(r1, r2, "相同输入应产生相同输出");
    }
}
