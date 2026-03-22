use lettre::message::header::ContentType;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::AppConfig;
use crate::error::AppError;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[derive(Clone)]
pub enum EmailService {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: String,
    },
    Log {
        from: String,
    },
}

impl EmailService {
    pub fn from_config(config: &AppConfig) -> Self {
        if let Some(host) = &config.smtp_host {
            match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host) {
                Ok(builder) => {
                    let mut builder = builder.port(config.smtp_port);

                    if let (Some(user), Some(password)) = (&config.smtp_user, &config.smtp_password)
                    {
                        builder =
                            builder.credentials(Credentials::new(user.clone(), password.clone()));
                    }

                    let transport = builder.build();

                    tracing::info!(
                        "Email service configured with SMTP: {host}:{}",
                        config.smtp_port
                    );
                    Self::Smtp {
                        transport,
                        from: config.smtp_from.clone(),
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create SMTP transport for host '{host}': {e} — falling back to log mode"
                    );
                    Self::Log {
                        from: config.smtp_from.clone(),
                    }
                }
            }
        } else {
            tracing::warn!("SMTP_HOST not set — emails will be logged to console (dev mode)");
            Self::Log {
                from: config.smtp_from.clone(),
            }
        }
    }

    pub async fn send_verification_email(
        &self,
        to_email: &str,
        display_name: &str,
        verification_token: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        let verify_url = format!("{base_url}/api/v1/auth/verify?token={verification_token}");

        let safe_name = html_escape(display_name);
        let subject = "Bestätige deine E-Mail-Adresse – LILLY";
        let body = format!(
            r#"<!DOCTYPE html>
<html lang="de">
<head><meta charset="utf-8"></head>
<body style="font-family: Inter, sans-serif; color: #1a1a1a;">
<h2>Hallo {safe_name},</h2>
<p>willkommen bei LILLY! Bitte bestätige deine E-Mail-Adresse, indem du auf den folgenden Link klickst:</p>
<p><a href="{verify_url}" style="display: inline-block; padding: 12px 24px; background-color: #06b6d4; color: white; text-decoration: none; border-radius: 8px;">E-Mail bestätigen</a></p>
<p>Oder kopiere diesen Link in deinen Browser:</p>
<p style="word-break: break-all;">{verify_url}</p>
<p>Dieser Link ist 24 Stunden gültig.</p>
<p>Falls du kein Konto bei LILLY erstellt hast, kannst du diese E-Mail ignorieren.</p>
<hr>
<p style="font-size: 12px; color: #888;">LILLY – Listing Inventory for Lovely Little Yellowbacks</p>
</body>
</html>"#
        );

        let sender = self.sender_address();

        let to_address: Address = to_email.parse().map_err(|e| {
            AppError::InternalError(anyhow::anyhow!("Invalid recipient email address: {e}"))
        })?;
        let recipient = Mailbox::new(Some(display_name.to_string()), to_address);

        match self {
            Self::Smtp { transport, .. } => {
                let email = Message::builder()
                    .from(sender.parse().map_err(|e| {
                        AppError::InternalError(anyhow::anyhow!("Invalid from address: {e}"))
                    })?)
                    .to(recipient)
                    .subject(subject)
                    .header(ContentType::TEXT_HTML)
                    .body(body)
                    .map_err(|e| {
                        AppError::InternalError(anyhow::anyhow!("Failed to build email: {e}"))
                    })?;

                transport.send(email).await.map_err(|e| {
                    tracing::error!("Failed to send verification email: {e}");
                    AppError::InternalError(anyhow::anyhow!("Failed to send email: {e}"))
                })?;

                tracing::info!("Verification email sent to {to_email}");
            }
            Self::Log { .. } => {
                tracing::info!(
                    "======== VERIFICATION EMAIL (dev mode) ========\n\
                     To: {to_email}\n\
                     Subject: {subject}\n\
                     Verify URL: {verify_url}\n\
                     ================================================"
                );
            }
        }

        Ok(())
    }

    fn sender_address(&self) -> &str {
        match self {
            Self::Smtp { from, .. } | Self::Log { from } => from,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_fallback_created_when_no_smtp_host() {
        let config = AppConfig {
            database_url: String::new(),
            jwt_secret: String::new(),
            jwt_access_token_expiry: 900,
            jwt_refresh_token_expiry: 2_592_000,
            backend_port: 8080,
            smtp_host: None,
            smtp_port: 587,
            smtp_user: None,
            smtp_password: None,
            smtp_from: "test@lilly.app".to_string(),
            app_base_url: "http://localhost".to_string(),
            cookie_secure: false,
            admin_email: None,
            media_path: "/media".to_string(),
            media_url_prefix: "/media".to_string(),
        };

        let service = EmailService::from_config(&config);
        assert!(matches!(service, EmailService::Log { .. }));
    }

    #[test]
    fn test_sender_address_returns_configured_from() {
        let service = EmailService::Log {
            from: "noreply@lilly.app".to_string(),
        };
        assert_eq!(service.sender_address(), "noreply@lilly.app");
    }

    #[tokio::test]
    async fn test_log_fallback_does_not_error() {
        let service = EmailService::Log {
            from: "noreply@lilly.app".to_string(),
        };
        let result = service
            .send_verification_email(
                "user@example.com",
                "Test User",
                "abc123token",
                "http://localhost",
            )
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_html_escape_special_characters() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn test_html_escape_ampersand_and_quotes() {
        assert_eq!(html_escape("A & B \"C\""), "A &amp; B &quot;C&quot;");
    }

    #[test]
    fn test_html_escape_plain_text_unchanged() {
        assert_eq!(html_escape("Max Mustermann"), "Max Mustermann");
    }

    #[tokio::test]
    async fn test_log_fallback_escapes_html_in_display_name() {
        let service = EmailService::Log {
            from: "noreply@lilly.app".to_string(),
        };
        // Should not error even with HTML characters in display_name
        let result = service
            .send_verification_email(
                "user@example.com",
                "<b>Evil</b>",
                "abc123token",
                "http://localhost",
            )
            .await;
        assert!(result.is_ok());
    }
}
