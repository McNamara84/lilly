use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Argon2id memory cost in KiB (19 MiB), the OWASP-recommended minimum profile.
pub const MEMORY_COST_KIB: u32 = 19_456;
/// Argon2id iteration count.
pub const TIME_COST: u32 = 2;
/// Argon2id degree of parallelism.
pub const PARALLELISM: u32 = 1;

fn argon2() -> Result<Argon2<'static>, argon2::password_hash::Error> {
    let params = Params::new(MEMORY_COST_KIB, TIME_COST, PARALLELISM, None)
        .map_err(|_| argon2::password_hash::Error::ParamInvalid { name: "argon2" })?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    // argon2 0.6 generates a random salt itself (default `getrandom` feature).
    let hash = argon2()?.hash_password(password.as_bytes())?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(argon2()?
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Whether a stored hash was created with weaker or different settings than the current policy
/// and should be replaced after the next successful authentication.
///
/// Anything that is not an Argon2id v19 hash with exactly the configured parameters counts as
/// outdated; an unparsable hash is left alone because it cannot be verified in the first place.
pub fn needs_rehash(hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    let Ok(params) = Params::try_from(&parsed) else {
        return true;
    };
    parsed.algorithm.as_str() != Algorithm::Argon2id.ident().as_str()
        || parsed.version != Some(Version::V0x13.into())
        || params.m_cost() != MEMORY_COST_KIB
        || params.t_cost() != TIME_COST
        || params.p_cost() != PARALLELISM
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
    fn new_hashes_use_the_configured_argon2id_parameters() {
        let hash = hash_password("policy-check-pass-2026!").expect("Failed to hash");

        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(!needs_rehash(&hash));
    }

    #[test]
    fn hashes_with_weaker_or_different_settings_need_a_rehash() {
        // Legacy hash from argon2 0.5.3 already matches the current policy.
        let current = "$argon2id$v=19$m=19456,t=2,p=1$50KPF7EqQptqJkfElIOyfA$y1NicTAED9UAZ7pfX3TK6eLGhsOGU4qp25zvhn6Y54Y";
        assert!(!needs_rehash(current));

        let weaker_memory = "$argon2id$v=19$m=4096,t=2,p=1$50KPF7EqQptqJkfElIOyfA$y1NicTAED9UAZ7pfX3TK6eLGhsOGU4qp25zvhn6Y54Y";
        let weaker_time = "$argon2id$v=19$m=19456,t=1,p=1$50KPF7EqQptqJkfElIOyfA$y1NicTAED9UAZ7pfX3TK6eLGhsOGU4qp25zvhn6Y54Y";
        let other_variant = "$argon2i$v=19$m=19456,t=2,p=1$50KPF7EqQptqJkfElIOyfA$y1NicTAED9UAZ7pfX3TK6eLGhsOGU4qp25zvhn6Y54Y";
        assert!(needs_rehash(weaker_memory));
        assert!(needs_rehash(weaker_time));
        assert!(needs_rehash(other_variant));
    }

    #[test]
    fn unparsable_hashes_are_not_flagged_for_rehash() {
        assert!(!needs_rehash("not-a-phc-string"));
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
