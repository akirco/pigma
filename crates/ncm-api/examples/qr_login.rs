use ncm_api::NcmClient;
use qrcode::render::unicode::Dense1x2;
use std::{thread, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NcmClient::new()?;

    let (qr_url, unikey) = client.login_qr_create().await?;

    println!("请用网易云音乐 App 扫描下方二维码登录：\n");
    let code = qrcode::QrCode::new(qr_url.as_bytes())?;
    let qr_art = code.render::<Dense1x2>().quiet_zone(false).build();
    println!("{}", qr_art);
    println!();

    let mut printed = false;
    loop {
        let resp = client.login_qr_check(&unikey).await?;
        match resp.code {
            803 => {
                println!("[OK] 登录成功！");
                break;
            }
            800 => {
                println!("[EXPIRED] 二维码已过期，请重新运行");
                return Ok(());
            }
            802 => {
                if !printed {
                    println!("[SCANNED] 已扫描，请在手机上确认...");
                    printed = true;
                }
            }
            _ => {
                if !printed {
                    println!("[WAITING] 等待扫码...");
                    printed = true;
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }

    let info = client.login_status().await?;
    println!("用户: {} (UID: {})", info.nickname, info.uid);
    client.flush_cookies();
    println!("Cookie 已保存");

    Ok(())
}
