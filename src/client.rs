use std::{
    io::{BufRead, BufReader, Cursor, Read, Write},
    net::TcpStream,
    sync::Arc,
};

use anyhow::{anyhow, bail, Result};
use rustls::{
    client::danger::{ServerCertVerified, ServerCertVerifier},
    crypto::{
        aws_lc_rs::default_provider, verify_tls12_signature, verify_tls13_signature, CryptoProvider,
    },
    ClientConfig,
};
use url::Url;

pub struct Client {
    client_config: Arc<ClientConfig>,
    auto_redirect: bool,
}

impl Client {
    pub fn new(auto_redirect: bool) -> Self {
        let root_store = rustls::RootCertStore { roots: Vec::new() };
        let mut config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(TofuCertVerifier::new(default_provider())));
        Self {
            client_config: Arc::new(config),
            auto_redirect,
        }
    }

    pub fn request(&self, mut url: Url) -> Result<GeminiResponse> {
        let port = url.port().unwrap_or(1965);
        if url.scheme() != "gemini" {
            return Err(anyhow!("Invalid scheme"));
        }
        if url.path().is_empty() {
            url.set_path("/");
        }
        let domain = url.domain().ok_or(anyhow!("Missing domain"))?;
        let mut conn = rustls::ClientConnection::new(
            self.client_config.clone(),
            domain.to_string().try_into()?,
        )?;
        let mut socket = TcpStream::connect(format!("{domain}:{port}"))?;
        let mut tls = rustls::Stream::new(&mut conn, &mut socket);
        tls.write_all(url.as_str().as_bytes())?;
        tls.write_all(b"\r\n")?;
        tls.flush()?;
        let mut read = BufReader::new(tls);
        let mut status = Vec::with_capacity(3);
        read.read_until(b' ', &mut status)?;
        let mut buffer = Vec::with_capacity(1024);
        read.take(1024 * 1024 * 8).read_to_end(&mut buffer)?;
        Ok(match status.as_slice() {
            b"10 " | b"11 " => {
                let status = InputStatus::try_from(status.as_slice())?;
                GeminiResponse::Input {
                    status,
                    prompt: String::from_utf8(buffer)?.trim().to_string(),
                }
            }
            b"20 " => {
                let mut cursor = Cursor::new(buffer);
                let mut header = String::new();
                let mut body = String::new();
                cursor.read_line(&mut header)?;
                cursor.read_to_string(&mut body)?;
                GeminiResponse::Success {
                    mime: header.trim().to_string(),
                    body: body.into(),
                }
            }
            b"30 " | b"31 " => {
                let status = RedirectStatus::try_from(status.as_slice())?;
                let url = Url::parse(String::from_utf8(buffer)?.trim())?;
                if self.auto_redirect {
                    return self.request(url);
                }
                GeminiResponse::Redirect { status, url }
            }
            b"40 " | b"41 " | b"42 " | b"43 " | b"44 " => {
                let status = TemporaryFailureStatus::try_from(status.as_slice())?;
                let error_msg = String::from_utf8(buffer)?;
                let trimmed = error_msg.trim();
                GeminiResponse::TemporaryFailure {
                    status,
                    error_msg: if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    },
                }
            }
            b"50 " | b"51 " | b"52 " | b"53 " | b"59 " => {
                let status = PermanentFailureStatus::try_from(status.as_slice())?;
                let error_msg = String::from_utf8(buffer)?;
                let trimmed = error_msg.trim();
                GeminiResponse::PermanentFailure {
                    status,
                    error_msg: if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    },
                }
            }
            b"60 " | b"61 " | b"62 " => {
                let status = ClientCertificateErrorStatus::try_from(status.as_slice())?;
                let error_msg = String::from_utf8(buffer)?;
                let trimmed = error_msg.trim();
                GeminiResponse::ClientCertificateError {
                    status,
                    error_msg: if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    },
                }
            }
            other => bail!("Invalid response code {}", String::from_utf8_lossy(other)),
        })
    }
}

#[derive(Debug, Clone)]
pub enum GeminiResponse {
    Input {
        status: InputStatus,
        prompt: String,
    },
    Success {
        mime: String,
        body: Vec<u8>,
    },
    Redirect {
        status: RedirectStatus,
        url: Url,
    },
    TemporaryFailure {
        status: TemporaryFailureStatus,
        error_msg: Option<String>,
    },
    PermanentFailure {
        status: PermanentFailureStatus,
        error_msg: Option<String>,
    },
    ClientCertificateError {
        status: ClientCertificateErrorStatus,
        error_msg: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum InputStatus {
    Normal,
    Sensitive,
}

impl TryFrom<&[u8]> for InputStatus {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match &value[0..2] {
            b"10" => InputStatus::Normal,
            b"11" => InputStatus::Sensitive,
            _ => bail!("Invalid input status"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum RedirectStatus {
    Temporary,
    Permanent,
}

impl TryFrom<&[u8]> for RedirectStatus {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match &value[0..2] {
            b"30" => RedirectStatus::Temporary,
            b"31" => RedirectStatus::Permanent,
            _ => bail!("Invalid input status"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum TemporaryFailureStatus {
    Unspecified,
    ServerUnavailable,
    CGIError,
    ProxyError,
    SlowDown,
}

impl TryFrom<&[u8]> for TemporaryFailureStatus {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match &value[0..2] {
            b"40" => TemporaryFailureStatus::Unspecified,
            b"41" => TemporaryFailureStatus::ServerUnavailable,
            b"42" => TemporaryFailureStatus::CGIError,
            b"43" => TemporaryFailureStatus::ProxyError,
            b"44" => TemporaryFailureStatus::SlowDown,
            _ => bail!("Invalid temporary failure status"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum PermanentFailureStatus {
    Unspecified,
    NotFound,
    Gone,
    ProxyRequestRefused,
    BadRequest,
}

impl TryFrom<&[u8]> for PermanentFailureStatus {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match &value[0..2] {
            b"50" => PermanentFailureStatus::Unspecified,
            b"51" => PermanentFailureStatus::NotFound,
            b"52" => PermanentFailureStatus::Gone,
            b"53" => PermanentFailureStatus::ProxyRequestRefused,
            b"59" => PermanentFailureStatus::BadRequest,
            _ => bail!("Invalid permanent failure status"),
        })
    }
}

#[derive(Debug, Clone)]
pub enum ClientCertificateErrorStatus {
    Required,
    NotAuthorized,
    NotValid,
}

impl TryFrom<&[u8]> for ClientCertificateErrorStatus {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match &value[0..2] {
            b"60" => ClientCertificateErrorStatus::Required,
            b"61" => ClientCertificateErrorStatus::NotAuthorized,
            b"62" => ClientCertificateErrorStatus::NotValid,
            _ => bail!("Invalid client certificate status"),
        })
    }
}

#[derive(Debug, Clone)]
struct TofuCertVerifier {
    provider: CryptoProvider,
}

impl TofuCertVerifier {
    pub fn new(provider: CryptoProvider) -> Self {
        Self { provider }
    }
}

/// We still need to actual store the cert in the first time and reutilize it afterwards
impl ServerCertVerifier for TofuCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
