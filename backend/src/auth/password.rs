use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    // argon2 0.6 generates a random salt itself (default `getrandom` feature).
    let hash = Argon2::default().hash_password(password.as_bytes())?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn validate_password_strength(
    password: &str,
    email: &str,
    display_name: &str,
) -> Result<(), &'static str> {
    let entropy = zxcvbn::zxcvbn(password, &[email, display_name]);
    if entropy.score() < zxcvbn::Score::Two {
        return Err("Password is too weak. Please choose a stronger password.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hashes stored before the argon2 0.6 upgrade must keep working. This PHC string was
    /// produced by argon2 0.5.3 with `Argon2::default()` and a random salt, exactly like the
    /// previous `hash_password` implementation.
    #[test]
    fn verifies_hashes_created_before_the_argon2_0_6_upgrade() {
        let legacy_hash = "$argon2id$v=19$m=19456,t=2,p=1$50KPF7EqQptqJkfElIOyfA$y1NicTAED9UAZ7pfX3TK6eLGhsOGU4qp25zvhn6Y54Y";

        assert!(verify_password("legacy-pass-2026!", legacy_hash).expect("legacy hash must parse"));
        assert!(!verify_password("wrong-password", legacy_hash).expect("legacy hash must parse"));
    }

    #[test]
    fn test_hash_and_verify_password() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("Failed to hash password");

        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(password, &hash).expect("Failed to verify"));
    }

    #[test]
    fn test_wrong_password_fails_verification() {
        let hash = hash_password("correct_password").expect("Failed to hash");
        let result = verify_password("wrong_password", &hash).expect("Failed to verify");
        assert!(!result);
    }

    #[test]
    fn test_different_hashes_for_same_password() {
        let password = "same_password";
        let hash1 = hash_password(password).expect("Failed to hash");
        let hash2 = hash_password(password).expect("Failed to hash");
        assert_ne!(hash1, hash2); // Different salts
    }

    #[test]
    fn password_strength_rejects_weak_and_personal_passwords() {
        assert!(validate_password_strength("password", "user@example.com", "User").is_err());
        assert!(
            validate_password_strength(
                "correct horse battery staple! 2049",
                "user@example.com",
                "User"
            )
            .is_ok()
        );
    }
}
