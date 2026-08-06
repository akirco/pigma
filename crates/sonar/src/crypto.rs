use crate::error::{Result, SonarError};
use base64::{Engine as _, engine::general_purpose};
use time::OffsetDateTime;

use md5;
use std::sync::OnceLock;
use urlencoding;

const KUGOU_KEY_SUFFIX: &str = "kgcloudv2";
const KUWO_DES_KEY: &[u8; 8] = b"ylzsxkwm";
const KUWO_SOURCE: &str = "kwplayer_ar_5.1.0.0_B_jiakong_vh.apk";

pub fn kugou_md5_key(hash: &str) -> String {
    let input = format!("{}{}", hash, KUGOU_KEY_SUFFIX);
    format!("{:x}", md5::compute(input))
}

// ---------------------------------------------------------------------------
// Kuwo "kwDES" — a custom DES variant used by Kuwo's mobile clients.
// Direct port of src/kwDES.js from UnblockNeteaseMusic (LSB-first bit order
// with byte-aligned S-box grouping). NOT compatible with standard DES, so a
// stock `des` crate cannot be used. Verified against known kwDES vectors.
// ---------------------------------------------------------------------------

const KUWO_E: [i16; 64] = [
    31, 0, 1, 2, 3, 4, -1, -1, 3, 4, 5, 6, 7, 8, -1, -1, 7, 8, 9, 10, 11, 12, -1, -1, 11, 12, 13,
    14, 15, 16, -1, -1, 15, 16, 17, 18, 19, 20, -1, -1, 19, 20, 21, 22, 23, 24, -1, -1, 23, 24, 25,
    26, 27, 28, -1, -1, 27, 28, 29, 30, 31, 30, -1, -1,
];

const KUWO_IP: [i16; 64] = [
    57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53, 45, 37, 29, 21, 13, 5, 63,
    55, 47, 39, 31, 23, 15, 7, 56, 48, 40, 32, 24, 16, 8, 0, 58, 50, 42, 34, 26, 18, 10, 2, 60, 52,
    44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
];

const KUWO_IP_1: [i16; 64] = [
    39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30, 37, 5, 45, 13, 53, 21, 61, 29,
    36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34, 2, 42, 10, 50, 18, 58, 26,
    33, 1, 41, 9, 49, 17, 57, 25, 32, 0, 40, 8, 48, 16, 56, 24,
];

const KUWO_LS: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

const KUWO_LS_MASK: [u64; 3] = [0, 0x100001, 0x300003];

const KUWO_P: [i16; 32] = [
    15, 6, 19, 20, 28, 11, 27, 16, 0, 14, 22, 25, 4, 17, 30, 9, 1, 7, 23, 13, 31, 26, 2, 8, 18, 12,
    29, 5, 21, 10, 3, 24,
];

const KUWO_PC_1: [i16; 56] = [
    56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59,
    51, 43, 35, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 60, 52, 44, 36, 28,
    20, 12, 4, 27, 19, 11, 3,
];

const KUWO_PC_2: [i16; 64] = [
    13, 16, 10, 23, 0, 4, -1, -1, 2, 27, 14, 5, 20, 9, -1, -1, 22, 18, 11, 3, 25, 7, -1, -1, 15, 6,
    26, 19, 12, 1, -1, -1, 40, 51, 30, 36, 46, 54, -1, -1, 29, 39, 50, 44, 32, 47, -1, -1, 43, 48,
    38, 55, 33, 52, -1, -1, 45, 41, 49, 35, 28, 31, -1, -1,
];

const KUWO_SBOX: [[u8; 64]; 8] = [
    [
        14, 4, 3, 15, 2, 13, 5, 3, 13, 14, 6, 9, 11, 2, 0, 5, 4, 1, 10, 12, 15, 6, 9, 10, 1, 8, 12,
        7, 8, 11, 7, 0, 0, 15, 10, 5, 14, 4, 9, 10, 7, 8, 12, 3, 13, 1, 3, 6, 15, 12, 6, 11, 2, 9,
        5, 0, 4, 2, 11, 14, 1, 7, 8, 13,
    ],
    [
        15, 0, 9, 5, 6, 10, 12, 9, 8, 7, 2, 12, 3, 13, 5, 2, 1, 14, 7, 8, 11, 4, 0, 3, 14, 11, 13,
        6, 4, 1, 10, 15, 3, 13, 12, 11, 15, 3, 6, 0, 4, 10, 1, 7, 8, 4, 11, 14, 13, 8, 0, 6, 2, 15,
        9, 5, 7, 1, 10, 12, 14, 2, 5, 9,
    ],
    [
        10, 13, 1, 11, 6, 8, 11, 5, 9, 4, 12, 2, 15, 3, 2, 14, 0, 6, 13, 1, 3, 15, 4, 10, 14, 9, 7,
        12, 5, 0, 8, 7, 13, 1, 2, 4, 3, 6, 12, 11, 0, 13, 5, 14, 6, 8, 15, 2, 7, 10, 8, 15, 4, 9,
        11, 5, 9, 0, 14, 3, 10, 7, 1, 12,
    ],
    [
        7, 10, 1, 15, 0, 12, 11, 5, 14, 9, 8, 3, 9, 7, 4, 8, 13, 6, 2, 1, 6, 11, 12, 2, 3, 0, 5,
        14, 10, 13, 15, 4, 13, 3, 4, 9, 6, 10, 1, 12, 11, 0, 2, 5, 0, 13, 14, 2, 8, 15, 7, 4, 15,
        1, 10, 7, 5, 6, 12, 11, 3, 8, 9, 14,
    ],
    [
        2, 4, 8, 15, 7, 10, 13, 6, 4, 1, 3, 12, 11, 7, 14, 0, 12, 2, 5, 9, 10, 13, 0, 3, 1, 11, 15,
        5, 6, 8, 9, 14, 14, 11, 5, 6, 4, 1, 3, 10, 2, 12, 15, 0, 13, 2, 8, 5, 11, 8, 0, 15, 7, 14,
        9, 4, 12, 7, 10, 9, 1, 13, 6, 3,
    ],
    [
        12, 9, 0, 7, 9, 2, 14, 1, 10, 15, 3, 4, 6, 12, 5, 11, 1, 14, 13, 0, 2, 8, 7, 13, 15, 5, 4,
        10, 8, 3, 11, 6, 10, 4, 6, 11, 7, 9, 0, 6, 4, 2, 13, 1, 9, 15, 3, 8, 15, 3, 1, 14, 12, 5,
        11, 0, 2, 12, 14, 7, 5, 10, 8, 13,
    ],
    [
        4, 1, 3, 10, 15, 12, 5, 0, 2, 11, 9, 6, 8, 7, 6, 9, 11, 4, 12, 15, 0, 3, 10, 5, 14, 13, 7,
        8, 13, 14, 1, 2, 13, 6, 14, 9, 4, 1, 2, 14, 11, 13, 5, 0, 1, 10, 8, 3, 0, 11, 3, 5, 9, 4,
        15, 2, 7, 8, 12, 15, 10, 7, 6, 12,
    ],
    [
        13, 7, 10, 0, 6, 9, 5, 15, 8, 4, 3, 10, 11, 14, 12, 5, 2, 11, 9, 6, 15, 12, 0, 3, 4, 1, 14,
        13, 1, 2, 7, 8, 1, 2, 12, 15, 10, 4, 0, 3, 13, 14, 6, 9, 7, 8, 9, 6, 15, 1, 5, 12, 3, 10,
        14, 5, 8, 7, 11, 0, 4, 13, 2, 11,
    ],
];

fn kuwo_bit_transform(table: &[i16], n: usize, input: u64) -> u64 {
    let mut output = 0u64;
    for (i, &idx) in table.iter().enumerate().take(n) {
        if idx < 0 || (input & (1u64 << idx)) == 0 {
            continue;
        }
        output |= 1u64 << i;
    }
    output
}

fn kuwo_subkeys(key: u64) -> [u64; 16] {
    let mut value = kuwo_bit_transform(&KUWO_PC_1, 56, key);
    let mut subkeys = [0u64; 16];
    for i in 0..16 {
        let shift = KUWO_LS[i];
        let mask = KUWO_LS_MASK[shift as usize];
        value = ((value & mask) << (28 - shift)) | ((value & !mask) >> shift);
        subkeys[i] = kuwo_bit_transform(&KUWO_PC_2, 64, value);
    }
    subkeys
}

fn kuwo_des64(subkeys: &[u64; 16], input: u64) -> u64 {
    let initial = kuwo_bit_transform(&KUWO_IP, 64, input);
    let mut p0 = initial & 0xFFFF_FFFF;
    let mut p1 = initial >> 32;

    let mut pr = [0u8; 8];
    for &sk in subkeys.iter() {
        let r = kuwo_bit_transform(&KUWO_E, 64, p1) ^ sk;
        for (j, slot) in pr.iter_mut().enumerate() {
            *slot = ((r >> (j * 8)) & 0xFF) as u8;
        }
        let mut s_out = 0u64;
        for sbi in (0..8).rev() {
            s_out = (s_out << 4) | u64::from(KUWO_SBOX[sbi][pr[sbi] as usize]);
        }
        let r2 = kuwo_bit_transform(&KUWO_P, 32, s_out);
        let l = p0;
        p0 = p1;
        p1 = l ^ r2;
    }

    let combined = (p0 << 32) | (p1 & 0xFFFF_FFFF);
    kuwo_bit_transform(&KUWO_IP_1, 64, combined)
}

fn kuwo_pack_bytes(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        value |= u64::from(b) << (i * 8);
    }
    value
}

fn kuwo_des64_bytes(subkeys: &[u64; 16], block: u64) -> [u8; 8] {
    let out = kuwo_des64(subkeys, block);
    let mut bytes = [0u8; 8];
    for (i, slot) in bytes.iter_mut().enumerate() {
        *slot = ((out >> (i * 8)) & 0xFF) as u8;
    }
    bytes
}

pub fn kuwo_des_encrypt(plaintext: &str) -> Result<String> {
    let msg = plaintext.as_bytes();
    let subkeys = kuwo_subkeys(kuwo_pack_bytes(KUWO_DES_KEY));
    let full_blocks = msg.len() / 8;

    let mut output = Vec::with_capacity((full_blocks + 1) * 8);
    for m in 0..full_blocks {
        let block = kuwo_pack_bytes(&msg[m * 8..m * 8 + 8]);
        output.extend_from_slice(&kuwo_des64_bytes(&subkeys, block));
    }

    let partial = kuwo_pack_bytes(&msg[full_blocks * 8..]);
    output.extend_from_slice(&kuwo_des64_bytes(&subkeys, partial));

    Ok(general_purpose::STANDARD.encode(output))
}

pub fn kuwo_build_query(rid: &str, format: &str) -> String {
    format!(
        "user=0&corp=kuwo&source={}&p2p=1&type=convert_url2&sig=0&format={}&rid=MUSIC_{}",
        KUWO_SOURCE, format, rid
    )
}

/* -------------------------------------------------------------------------- */
/*                                  bilibili                                  */
/* -------------------------------------------------------------------------- */

static WBI_KEYS: OnceLock<(String, String)> = OnceLock::new();

pub async fn get_wbi_keys() -> Result<(String, String)> {
    if let Some(keys) = WBI_KEYS.get() {
        return Ok(keys.clone());
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    let img_url = json["data"]["wbi_img"]["img_url"]
        .as_str()
        .ok_or_else(|| SonarError::WbiSign("Missing img_url".into()))?;
    let sub_url = json["data"]["wbi_img"]["sub_url"]
        .as_str()
        .ok_or_else(|| SonarError::WbiSign("Missing sub_url".into()))?;

    let img_key = img_url
        .split('/')
        .next_back()
        .and_then(|s| s.split('.').next())
        .ok_or_else(|| SonarError::WbiSign("Invalid img_url".into()))?
        .to_string();
    let sub_key = sub_url
        .split('/')
        .next_back()
        .and_then(|s| s.split('.').next())
        .ok_or_else(|| SonarError::WbiSign("Invalid sub_url".into()))?
        .to_string();

    let _ = WBI_KEYS.set((img_key.clone(), sub_key.clone()));
    Ok((img_key, sub_key))
}

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

fn get_mixin_key(orig: &str) -> String {
    MIXIN_KEY_ENC_TAB
        .iter()
        .map(|&i| orig.chars().nth(i).unwrap_or('0'))
        .collect::<String>()[..32]
        .to_string()
}

pub async fn wbi_sign(params: &mut Vec<(String, String)>) -> Result<String> {
    let (img_key, sub_key) = get_wbi_keys().await?;
    let mixin_key = get_mixin_key(&format!("{}{}", img_key, sub_key));

    let curr_time = OffsetDateTime::now_utc().unix_timestamp().to_string();

    params.push(("wts".to_string(), curr_time));

    params.sort_by(|a, b| a.0.cmp(&b.0));

    let query = params
        .iter()
        .map(|(k, v)| {
            let k = urlencoding::encode(k);
            let cleaned = v.replace(|c: char| "!'()*".contains(c), "");
            let v = urlencoding::encode(&cleaned);
            format!("{}={}", k, v)
        })
        .collect::<Vec<_>>()
        .join("&");

    let sign_input = format!("{}{}", query, mixin_key);
    let w_rid = format!("{:x}", md5::compute(sign_input));

    Ok(format!("{}&w_rid={}", query, w_rid))
}

#[cfg(test)]
mod tests {
    use super::kuwo_des_encrypt;

    // Expected values generated from the reference UnblockNeteaseMusic kwDES.js.
    #[test]
    fn kuwo_des_encrypt_matches_reference() {
        let cases = [
            ("", "amNIiFPkHdE="),
            ("a", "Gm3y+RY0NdU="),
            ("1234567", "p0osj2O8+7U="),
            ("12345678", "Njw+DTAxpGxqY0iIU+Qd0Q=="),
            ("123456789", "Njw+DTAxpGwIIc44uKhZLQ=="),
            ("12345678abcdefgh", "Njw+DTAxpGyuQK/nPnXP6WpjSIhT5B3R"),
            (
                "user=0&corp=kuwo&source=kwplayer_ar_5.1.0.0_B_jiakong_vh.apk&p2p=1&type=convert_url2&sig=0&format=mp3&rid=MUSIC_32511210",
                "3HxQnWXTNdQ6RbicYxLOyHAu64fVpKoBz43BshH4RFaGBPBi+8dZdGuvz4Hu9TAfA75CH9prKR/wLP/IiYIvJoWxgCvU/gETNIqiGvqcuuscRcbgESVmpm7oNjCqzuWEIZJuWAS1Zw43FNLCKKA7IVLYW6bkBZ3PamNIiFPkHdE=",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                kuwo_des_encrypt(input).unwrap(),
                expected,
                "input: {:?}",
                input
            );
        }
    }
}
