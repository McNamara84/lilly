use sqlx::MySqlPool;

use crate::models::user::User;

pub async fn find_user_by_email(pool: &MySqlPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, display_name FROM users WHERE email = ?"
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn seed_demo_user(pool: &MySqlPool) {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));

    if count.0 == 0 {
        let password_hash = crate::auth::password::hash_password("demo1234")
            .expect("Failed to hash demo password");

        sqlx::query(
            "INSERT INTO users (email, password_hash, display_name) VALUES (?, ?, ?)"
        )
        .bind("demo@lilly.app")
        .bind(&password_hash)
        .bind("Demo-Sammler")
        .execute(pool)
        .await
        .expect("Failed to seed demo user");

        tracing::info!("Demo user seeded: demo@lilly.app / demo1234");
    }
}
