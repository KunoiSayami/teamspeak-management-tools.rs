pub mod v1 {
    use tap::TapFallible;
    use totp_rs::{Algorithm, Secret, TotpUrlError, TOTP};

    const TOTP_ALGORITHM: Algorithm = Algorithm::SHA1;
    const TOTP_DIGITS: usize = 8;
    const TOTP_SKEW: u8 = 1;
    const TOTP_STEP: u64 = 30;
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
        println!("totp = \"{}\"", totp.get_url());
        qr2term::print_qr(totp.get_url())
            .tap_err(|e| eprintln!("Unable print QR code, skipped: {:?}", e))
            .ok();
    }

    pub fn verify_totp_url(url: &String, code: &String) -> Result<bool, TotpUrlError> {
        Ok(TOTP::from_url(url)?
            .check_current(&code)
            .expect("System time error"))
    }

    pub fn show_code(url: &String) -> Result<String, TotpUrlError> {
        Ok(TOTP::from_url(url)?
            .generate_current()
            .expect("System time error"))
    }
}

fn check_totp(config: &Config) -> &String {
    match config.server().totp() {
        None => {
            panic!("You should specify url in your configure, if you don't have one, use `new' subcommand.");
        }
        Some(url) => url,
    }
}

pub fn show_totp_code(config: Config) -> Result<(), totp_rs::TotpUrlError> {
    println!("{}", current::show_code(check_totp(&config))?);
    Ok(())
}

pub fn verify_totp_code(config: Config, code: &String) -> Result<(), totp_rs::TotpUrlError> {
    println!(
        "result: {:?}",
        current::verify_totp_url(check_totp(&config), code)?
    );
    Ok(())
}

use crate::datastructures::Config;
pub use v1 as current;
