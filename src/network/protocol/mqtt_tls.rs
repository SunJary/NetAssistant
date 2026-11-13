//! MQTT TLS 传输配置（rustls）。
//!
//! 统一走「自建 `rustls::ClientConfig` → `TlsConfiguration::Rustls`」：
//! - `TlsConfiguration::Simple` 在 `ca` 为空时直接报错，无法表达「系统根证书」；
//! - 自建还能统一处理跳过校验、ALPN 与双向认证。
//!
//! **注意**：本仓库依赖树中的 rustls 同时启用了 `aws-lc-rs` 与 `ring` 两个
//! crypto provider，`ClientConfig::builder()` 在没有进程级默认 provider 时会 panic。
//! 因此这里始终显式 `builder_with_provider(aws_lc_rs)`，且不修改全局默认 provider
//! （避免影响 reqwest 的既有行为）。

use std::fs;
use std::io::Cursor;
use std::sync::Arc;

use rumqttc::{TlsConfiguration, Transport};
use rust_i18n::t;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, aws_lc_rs};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::config::connection::{TlsSettings, TlsVerifyMode};

/// 由配置构造 rumqttc TLS 传输（错误为可直接展示给用户的本地化文案）。
pub fn build_transport(tls: &TlsSettings) -> Result<Transport, String> {
    let provider = Arc::new(aws_lc_rs::default_provider());
    let roots = build_roots(tls)?;

    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| t!("mqtt_net.tls_build_failed", reason => e.to_string()).to_string())?
        .with_root_certificates(roots);

    let mut config = match load_client_auth(tls)? {
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .map_err(|e| t!("mqtt_net.tls_build_failed", reason => e.to_string()).to_string())?,
        None => builder.with_no_client_auth(),
    };

    if let Some(alpn) = alpn_protocols(tls) {
        config.alpn_protocols = alpn;
    }
    if tls.verify == TlsVerifyMode::Insecure {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoVerify(provider)));
    }

    Ok(Transport::tls_with_config(TlsConfiguration::Rustls(
        Arc::new(config),
    )))
}

/// 构造根证书库：系统根 / 自定义 CA / 跳过校验（空库）。
fn build_roots(tls: &TlsSettings) -> Result<RootCertStore, String> {
    let mut roots = RootCertStore::empty();
    match tls.verify {
        TlsVerifyMode::Insecure => {}
        TlsVerifyMode::SystemRoots => {
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = roots.add(cert);
            }
            if roots.is_empty() {
                return Err(t!("mqtt_net.tls_root_certs_unavailable").to_string());
            }
        }
        TlsVerifyMode::CustomCa => {
            let path = tls
                .ca_path
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| t!("mqtt_net.tls_ca_required").to_string())?;
            let bytes = read_file(path)?;
            let mut reader = Cursor::new(&bytes[..]);
            let certs = rustls_pemfile::certs(&mut reader)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| t!("mqtt_net.tls_ca_parse_failed", path => path.to_string(), reason => e.to_string()).to_string())?;
            roots.add_parsable_certificates(certs);
            if roots.is_empty() {
                return Err(t!("mqtt_net.tls_ca_empty", path => path.to_string()).to_string());
            }
        }
    }
    Ok(roots)
}

/// 读取客户端证书与私钥（双向认证）；两者必须成对出现。
fn load_client_auth(
    tls: &TlsSettings,
) -> Result<Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>, String> {
    let cert_path = tls
        .client_cert_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let key_path = tls
        .client_key_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    let (Some(cert_path), Some(key_path)) = (cert_path, key_path) else {
        return if cert_path.is_none() && key_path.is_none() {
            Ok(None)
        } else {
            Err(t!("mqtt_net.tls_client_auth_incomplete").to_string())
        };
    };

    let cert_bytes = read_file(cert_path)?;
    let mut cert_reader = Cursor::new(&cert_bytes[..]);
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| t!("mqtt_net.tls_cert_parse_failed", path => cert_path.to_string(), reason => e.to_string()).to_string())?;
    if certs.is_empty() {
        return Err(t!("mqtt_net.tls_cert_empty", path => cert_path.to_string()).to_string());
    }

    let key_bytes = read_file(key_path)?;
    let mut key_reader = Cursor::new(&key_bytes[..]);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| t!("mqtt_net.tls_key_parse_failed", path => key_path.to_string(), reason => e.to_string()).to_string())?
        .ok_or_else(|| t!("mqtt_net.tls_key_empty", path => key_path.to_string()).to_string())?;

    Ok(Some((certs, key)))
}

/// ALPN 列表：逗号分隔（也接受中文逗号），逐项转字节。
fn alpn_protocols(tls: &TlsSettings) -> Option<Vec<Vec<u8>>> {
    let raw = tls.alpn.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let list: Vec<Vec<u8>> = raw
        .split([',', '，'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.as_bytes().to_vec())
        .collect();
    if list.is_empty() { None } else { Some(list) }
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| {
        t!("mqtt_net.tls_file_read_failed", path => path.to_string(), reason => e.to_string())
            .to_string()
    })
}

/// 「跳过校验」用的校验器：接受任意证书与主机名（仍做握手签名校验）。
#[derive(Debug)]
struct NoVerify(Arc<CryptoProvider>);

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(verify: TlsVerifyMode) -> TlsSettings {
        TlsSettings {
            enabled: true,
            verify,
            ..TlsSettings::default()
        }
    }

    #[test]
    fn insecure_mode_builds_tls_transport() {
        let transport =
            build_transport(&settings(TlsVerifyMode::Insecure)).expect("跳过校验应可构造");
        assert!(matches!(transport, Transport::Tls(_)));
    }

    #[test]
    fn custom_ca_requires_path() {
        let err = expect_build_error(settings(TlsVerifyMode::CustomCa));
        assert!(!err.is_empty());
    }

    #[test]
    fn custom_ca_reports_missing_file() {
        let mut tls = settings(TlsVerifyMode::CustomCa);
        tls.ca_path = Some("Z:/definitely/not/here/ca.pem".to_string());
        let err = expect_build_error(tls);
        assert!(err.contains("ca.pem"), "错误文案应包含具体路径: {err}");
    }

    #[test]
    fn client_auth_requires_cert_and_key_in_pair() {
        let mut tls = settings(TlsVerifyMode::Insecure);
        tls.client_cert_path = Some("client.pem".to_string());
        let err = expect_build_error(tls);
        assert!(!err.is_empty());
    }

    /// `Transport` 不实现 Debug，不能用 `expect_err`，这里手动解包。
    fn expect_build_error(tls: TlsSettings) -> String {
        match build_transport(&tls) {
            Ok(_) => panic!("该配置应当构造失败"),
            Err(err) => err,
        }
    }

    #[test]
    fn alpn_is_split_and_trimmed() {
        let mut tls = settings(TlsVerifyMode::Insecure);
        tls.alpn = Some(" mqtt , mqttv5 ".to_string());
        assert_eq!(
            alpn_protocols(&tls),
            Some(vec![b"mqtt".to_vec(), b"mqttv5".to_vec()])
        );

        tls.alpn = Some("   ".to_string());
        assert_eq!(alpn_protocols(&tls), None);
    }
}
