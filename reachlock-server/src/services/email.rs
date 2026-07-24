//! S51: email sending abstraction. Noop/File/Smtp backends.
//! Mailpit in docker-compose for local dev at smtp://127.0.0.1:1025.

use std::path::PathBuf;
use std::sync::Mutex;

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

pub trait EmailBackend: Send + Sync {
    fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String>;
}

/// Logs to tracing, never sends. Default for dev without SMTP config.
pub struct NoopEmailBackend;

impl EmailBackend for NoopEmailBackend {
    fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String> {
        tracing::info!(target: "email", to, subject, "would send email (noop backend)");
        tracing::debug!(target: "email", body = html_body, "email body");
        Ok(())
    }
}

/// Writes `.eml` files to a directory. Mailpit-compatible for local dev.
pub struct FileEmailBackend {
    dir: PathBuf,
    counter: Mutex<u64>,
}

impl FileEmailBackend {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        FileEmailBackend {
            dir,
            counter: Mutex::new(0),
        }
    }
}

impl EmailBackend for FileEmailBackend {
    fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String> {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        let path = self.dir.join(format!("{:04}_{}.eml", c, chrono::Utc::now().format("%H%M%S")));
        let eml = format!(
            "To: {}\nSubject: {}\nContent-Type: text/html; charset=utf-8\n\n{}",
            to, subject, html_body
        );
        std::fs::write(&path, &eml).map_err(|e| format!("write email: {e}"))?;
        tracing::info!(target: "email", to, subject, path = %path.display(), "wrote email file");
        Ok(())
    }
}

/// SMTP via lettre. Production backend.
pub struct SmtpEmailBackend {
    sender: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl SmtpEmailBackend {
    pub fn new(smtp_url: &str, from: &str) -> Result<Self, String> {
        let rest = smtp_url
            .strip_prefix("smtp://")
            .ok_or_else(|| "SMTP URL must start with smtp://".to_string())?;

        let (userinfo, hostport) = match rest.split_once('@') {
            Some((ui, hp)) => (Some(ui), hp),
            None => (None, rest),
        };

        let (host, port_str) = hostport.split_once(':').unwrap_or((hostport, "587"));
        let port: u16 = port_str.parse().map_err(|_| "bad port".to_string())?;

        let (username, password) = match userinfo {
            Some(ui) => match ui.split_once(':') {
                Some((u, p)) => (u.to_string(), p.to_string()),
                None => (ui.to_string(), String::new()),
            },
            None => (String::new(), String::new()),
        };

        let creds = if !username.is_empty() {
            Some(Credentials::new(username, password))
        } else {
            None
        };

        // Use builder_dangerous for maximum compatibility. TLS is handled
        // by STARTTLS (auto-negotiated by relay()) or by an external proxy.
        // Port 587: use relay() which negotiates STARTTLS.
        // Port 25/1025: no TLS (local dev with Mailpit).
        let mailer = if port == 25 || port == 1025 {
            // No TLS for local dev
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                .port(port)
        } else {
            // Use relay() which auto-negotiates STARTTLS
            AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|e| format!("SMTP relay: {e}"))?
                .port(port)
        };
        let mailer = if let Some(c) = creds {
            mailer.credentials(c)
        } else {
            mailer
        };

        Ok(SmtpEmailBackend {
            sender: mailer.build(),
            from: from.to_string(),
        })
    }
}

impl EmailBackend for SmtpEmailBackend {
    fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String> {
        let email = Message::builder()
            .from(self.from.parse().map_err(|e: lettre::address::AddressError| format!("from addr: {e}"))?)
            .to(to.parse().map_err(|e: lettre::address::AddressError| format!("to addr: {e}"))?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string())
            .map_err(|e| format!("email body: {e}"))?;

        let rt = tokio::runtime::Handle::current();
        rt.block_on(async { self.sender.send(email).await })
            .map_err(|e| format!("send email: {e}"))?;
        tracing::info!(target: "email", to, subject, "sent via SMTP");
        Ok(())
    }
}
