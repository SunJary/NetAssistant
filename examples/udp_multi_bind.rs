//! 诊断样例: 复现"多个实例监听同一 UDP 端口"的套接字层行为。
//!
//! 用法:
//!   cargo run --example udp_multi_bind -- a 19901     # 进程A: bind 0.0.0.0:port 后收包6秒
//!   cargo run --example udp_multi_bind -- b 19901     # 进程B: 同端口 bind 0.0.0.0
//!   cargo run --example udp_multi_bind -- blocal 19901 # 进程B: bind 127.0.0.1:port
//!   cargo run --example udp_multi_bind -- send 19901  # 向 127.0.0.1:port 发 3 个数据报
//!
//! 与 UdpServer::start 相同: tokio UdpSocket::bind + recv_from 循环(读错误即退出)。
use std::time::Duration;

use tokio::net::UdpSocket;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let role = args.get(1).map(|s| s.as_str()).unwrap_or("a");
    let port: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(19901);

    if role == "send" {
        let sock = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        for i in 0..3u32 {
            let n = sock
                .send_to(format!("ping-{i}").as_bytes(), format!("127.0.0.1:{port}"))
                .await
                .unwrap_or(0);
            println!("[send] -> 127.0.0.1:{port} {n} bytes (ping-{i})");
        }
        return;
    }

    let bind_addr = match role {
        "blocal" => format!("127.0.0.1:{port}"),
        _ => format!("0.0.0.0:{port}"),
    };

    match UdpSocket::bind(&bind_addr).await {
        Ok(sock) => {
            println!("[{role}] BIND_OK {bind_addr}");
            let mut buf = [0u8; 1024];
            loop {
                match tokio::time::timeout(Duration::from_secs(6), sock.recv_from(&mut buf)).await {
                    Ok(Ok((n, addr))) => println!(
                        "[{role}] RECV {n} bytes from {addr}: {:?}",
                        String::from_utf8_lossy(&buf[..n])
                    ),
                    Ok(Err(e)) => {
                        println!("[{role}] RECV_ERR {e:?}");
                        break;
                    }
                    Err(_) => {
                        println!("[{role}] TIMEOUT: 6s 内未收到任何数据报");
                        break;
                    }
                }
            }
        }
        Err(e) => println!("[{role}] BIND_ERR: {e}"),
    }
}
