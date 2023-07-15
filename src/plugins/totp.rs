pub mod v1 {
    use totp_rs::{Algorithm, Secret, TOTP};
    const TOTP_ALGORITHM: Algorithm = Algorithm::SHA1;
    const TOTP_DIGITS: usize = 8;
    const TOTP_SKEW: u8 = 1;
    const TOTP_STEP: u64 = 10;
    const TOTP_PROVIDER: &str = "teamspeak-management-tools";
    const TOTP_ACCOUNT: &str = "server_admin";

    pub fn generate_new_totp() {
        let totp = TOTP::new(
            TOTP_ALGORITHM,
            TOTP_DIGITS,
            TOTP_SKEW,
            TOTP_STEP,
            Secret::generate_secret().to_bytes().unwrap(),
            Some(std::env::var("TOTP_PROVIDER").unwrap_or_else(|_| TOTP_PROVIDER.to_string())),
            TOTP_ACCOUNT.to_string(),
        )
        .unwrap();
        println!("{}", totp.get_url());
        let code = totp.get_qr().unwrap();
        println!("{}", code);
    }

    pub fn verify_totp(secret: String, code: String) -> Result<bool, Box<dyn std::error::Error>> {
        let totp = TOTP::new(
            TOTP_ALGORITHM,
            TOTP_DIGITS,
            TOTP_SKEW,
            TOTP_STEP,
            secret.into_bytes(),
            None,
            TOTP_ACCOUNT.to_string(),
        )?;
        Ok(totp.check_current(&code)?)
    }
}

pub use v1 as current;
