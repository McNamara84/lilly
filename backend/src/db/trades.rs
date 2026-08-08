use sqlx::MySqlPool;

use crate::models::trades::{
    BulkWantedResponse, TradeListEntryRow, TradeListQueryParams, WantedCandidateRow,
    WantedMutationResult, WantedRejection,
};

pub async fn find_trade_list_entries(
    pool: &MySqlPool,
    user_id: u32,
    status: &str,
    params: &TradeListQueryParams,
) -> Result<Vec<TradeListEntryRow>, sqlx::Error> {
    let series_slug = params.series_slug();
    let search = params.search();

    sqlx::query_as::<_, TradeListEntryRow>(
        "SELECT ce.id AS entry_id, ce.issue_id, i.issue_number, i.title,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path, ce.copy_number, ce.condition_grade,
                u.id AS owner_id, u.display_name AS owner_display_name
         FROM collection_entries ce
         JOIN issues i ON i.id = ce.issue_id
         JOIN series s ON s.id = i.series_id
         JOIN users u ON u.id = ce.user_id
         WHERE ce.user_id = ? AND ce.status = ? AND s.active = TRUE
           AND (? IS NULL OR s.slug = ?)
           AND (? IS NULL OR i.title LIKE CONCAT('%', ?, '%')
                OR EXISTS (
                    SELECT 1 FROM issue_persons ip
                    JOIN persons p ON p.id = ip.person_id
                    WHERE ip.issue_id = i.id AND ip.role = 'author'
                      AND p.name LIKE CONCAT('%', ?, '%')
                ))
         ORDER BY s.name ASC, i.issue_number ASC, ce.copy_number ASC, ce.id ASC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(status)
    .bind(series_slug)
    .bind(series_slug)
    .bind(search)
    .bind(search)
    .bind(search)
    .bind(params.per_page())
    .bind(params.offset())
    .fetch_all(pool)
    .await
}

pub async fn count_trade_list_entries(
    pool: &MySqlPool,
    user_id: u32,
    status: &str,
    params: &TradeListQueryParams,
) -> Result<u32, sqlx::Error> {
    let series_slug = params.series_slug();
    let search = params.search();
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM collection_entries ce
         JOIN issues i ON i.id = ce.issue_id
         JOIN series s ON s.id = i.series_id
         WHERE ce.user_id = ? AND ce.status = ? AND s.active = TRUE
           AND (? IS NULL OR s.slug = ?)
           AND (? IS NULL OR i.title LIKE CONCAT('%', ?, '%')
                OR EXISTS (
                    SELECT 1 FROM issue_persons ip
                    JOIN persons p ON p.id = ip.person_id
                    WHERE ip.issue_id = i.id AND ip.role = 'author'
                      AND p.name LIKE CONCAT('%', ?, '%')
                ))",
    )
    .bind(user_id)
    .bind(status)
    .bind(series_slug)
    .bind(series_slug)
    .bind(search)
    .bind(search)
    .bind(search)
    .fetch_one(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

pub async fn find_wanted_candidates(
    pool: &MySqlPool,
    user_id: u32,
    params: &TradeListQueryParams,
) -> Result<Vec<WantedCandidateRow>, sqlx::Error> {
    let series_slug = params.series_slug().unwrap_or_default();
    let search = params.search();

    sqlx::query_as::<_, WantedCandidateRow>(
        "SELECT i.id AS issue_id, i.issue_number, i.title,
                s.id AS series_id, s.name AS series_name, s.slug AS series_slug,
                i.cover_url, i.cover_local_path,
                (SELECT MIN(w.id) FROM collection_entries w
                 WHERE w.user_id = ? AND w.issue_id = i.id AND w.status = 'wanted')
                    AS wanted_entry_id
         FROM issues i
         JOIN series s ON s.id = i.series_id
         WHERE s.slug = ? AND s.active = TRUE
           AND NOT EXISTS (
               SELECT 1 FROM collection_entries owned
               WHERE owned.user_id = ? AND owned.issue_id = i.id
                 AND owned.status IN ('owned', 'duplicate')
           )
           AND (? IS NULL OR i.title LIKE CONCAT('%', ?, '%')
                OR EXISTS (
                    SELECT 1 FROM issue_persons ip
                    JOIN persons p ON p.id = ip.person_id
                    WHERE ip.issue_id = i.id AND ip.role = 'author'
                      AND p.name LIKE CONCAT('%', ?, '%')
                ))
         ORDER BY i.issue_number ASC, i.id ASC
         LIMIT ? OFFSET ?",
    )
    .bind(user_id)
    .bind(series_slug)
    .bind(user_id)
    .bind(search)
    .bind(search)
    .bind(search)
    .bind(params.per_page())
    .bind(params.offset())
    .fetch_all(pool)
    .await
}

pub async fn count_wanted_candidates(
    pool: &MySqlPool,
    user_id: u32,
    params: &TradeListQueryParams,
) -> Result<u32, sqlx::Error> {
    let series_slug = params.series_slug().unwrap_or_default();
    let search = params.search();
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM issues i
         JOIN series s ON s.id = i.series_id
         WHERE s.slug = ? AND s.active = TRUE
           AND NOT EXISTS (
               SELECT 1 FROM collection_entries owned
               WHERE owned.user_id = ? AND owned.issue_id = i.id
                 AND owned.status IN ('owned', 'duplicate')
           )
           AND (? IS NULL OR i.title LIKE CONCAT('%', ?, '%')
                OR EXISTS (
                    SELECT 1 FROM issue_persons ip
                    JOIN persons p ON p.id = ip.person_id
                    WHERE ip.issue_id = i.id AND ip.role = 'author'
                      AND p.name LIKE CONCAT('%', ?, '%')
                ))",
    )
    .bind(series_slug)
    .bind(user_id)
    .bind(search)
    .bind(search)
    .bind(search)
    .fetch_one(pool)
    .await?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(count as u32)
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingEntryStatus {
    id: u32,
    status: String,
}

#[derive(Debug, PartialEq, Eq)]
enum WantedDecision {
    Create,
    Unchanged(u32),
    RejectAlreadyOwned,
}

fn decide_wanted_mutation(entries: &[ExistingEntryStatus]) -> WantedDecision {
    if entries
        .iter()
        .any(|entry| matches!(entry.status.as_str(), "owned" | "duplicate"))
    {
        return WantedDecision::RejectAlreadyOwned;
    }

    entries
        .iter()
        .find(|entry| entry.status == "wanted")
        .map_or(WantedDecision::Create, |entry| {
            WantedDecision::Unchanged(entry.id)
        })
}

pub async fn add_wanted_entries(
    pool: &MySqlPool,
    user_id: u32,
    issue_ids: &[u32],
) -> Result<BulkWantedResponse, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let mut response = BulkWantedResponse::default();

    for &issue_id in issue_ids {
        // Lock the issue row so concurrent bulk requests serialize per issue.
        // Normalized issue IDs are sorted, keeping the lock order stable.
        let active_issue_id = sqlx::query_scalar::<_, u32>(
            "SELECT i.id FROM issues i
             JOIN series s ON s.id = i.series_id
             WHERE i.id = ? AND s.active = TRUE
             FOR UPDATE",
        )
        .bind(issue_id)
        .fetch_optional(&mut *transaction)
        .await?;

        if active_issue_id.is_none() {
            response.rejected.push(WantedRejection {
                issue_id,
                reason: "issue_not_found",
            });
            continue;
        }

        let entries = sqlx::query_as::<_, ExistingEntryStatus>(
            "SELECT id, status FROM collection_entries
             WHERE user_id = ? AND issue_id = ?
             ORDER BY copy_number ASC
             FOR UPDATE",
        )
        .bind(user_id)
        .bind(issue_id)
        .fetch_all(&mut *transaction)
        .await?;

        match decide_wanted_mutation(&entries) {
            WantedDecision::RejectAlreadyOwned => response.rejected.push(WantedRejection {
                issue_id,
                reason: "already_owned",
            }),
            WantedDecision::Unchanged(entry_id) => {
                response
                    .unchanged
                    .push(WantedMutationResult { issue_id, entry_id });
            }
            WantedDecision::Create => {
                let result = sqlx::query(
                    "INSERT INTO collection_entries
                        (user_id, issue_id, copy_number, condition_grade, status, notes)
                     VALUES (?, ?, 1, NULL, 'wanted', NULL)",
                )
                .bind(user_id)
                .bind(issue_id)
                .execute(&mut *transaction)
                .await;

                match result {
                    Ok(result) => {
                        #[allow(clippy::cast_possible_truncation)]
                        let entry_id = result.last_insert_id() as u32;
                        response
                            .created
                            .push(WantedMutationResult { issue_id, entry_id });
                    }
                    Err(error)
                        if matches!(
                            &error,
                            sqlx::Error::Database(database_error)
                                if database_error.kind()
                                    == sqlx::error::ErrorKind::UniqueViolation
                        ) =>
                    {
                        let concurrent_entries = sqlx::query_as::<_, ExistingEntryStatus>(
                            "SELECT id, status FROM collection_entries
                             WHERE user_id = ? AND issue_id = ?
                             ORDER BY copy_number ASC",
                        )
                        .bind(user_id)
                        .bind(issue_id)
                        .fetch_all(&mut *transaction)
                        .await?;

                        match decide_wanted_mutation(&concurrent_entries) {
                            WantedDecision::Unchanged(entry_id) => {
                                response
                                    .unchanged
                                    .push(WantedMutationResult { issue_id, entry_id });
                            }
                            WantedDecision::RejectAlreadyOwned => {
                                response.rejected.push(WantedRejection {
                                    issue_id,
                                    reason: "already_owned",
                                });
                            }
                            WantedDecision::Create => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }

    transaction.commit().await?;
    Ok(response)
}

pub async fn delete_wanted_entry(
    pool: &MySqlPool,
    user_id: u32,
    entry_id: u32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM collection_entries
         WHERE id = ? AND user_id = ? AND status = 'wanted'",
    )
    .bind(entry_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use sqlx::mysql::MySqlPoolOptions;

    use super::*;

    fn entry(id: u32, status: &str) -> ExistingEntryStatus {
        ExistingEntryStatus {
            id,
            status: status.to_string(),
        }
    }

    #[test]
    fn wanted_mutation_creates_without_existing_entries() {
        assert_eq!(decide_wanted_mutation(&[]), WantedDecision::Create);
    }

    #[test]
    fn wanted_mutation_is_idempotent_for_existing_wanted_entry() {
        assert_eq!(
            decide_wanted_mutation(&[entry(7, "wanted")]),
            WantedDecision::Unchanged(7)
        );
    }

    #[test]
    fn wanted_mutation_rejects_any_owned_or_duplicate_copy() {
        assert_eq!(
            decide_wanted_mutation(&[entry(7, "wanted"), entry(8, "owned")]),
            WantedDecision::RejectAlreadyOwned
        );
        assert_eq!(
            decide_wanted_mutation(&[entry(9, "duplicate")]),
            WantedDecision::RejectAlreadyOwned
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn trade_queries_and_wanted_mutations_work_against_mariadb() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("test database must be reachable");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("test migrations must succeed");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let first_user_id = insert_user(
            &pool,
            &format!("trade-owner-{suffix}@example.test"),
            "Trade Owner",
        )
        .await;
        let second_user_id = insert_user(
            &pool,
            &format!("trade-other-{suffix}@example.test"),
            "Other Collector",
        )
        .await;
        let active_series_id = insert_series(
            &pool,
            &format!("Trade Test {suffix}"),
            &format!("trade-test-{suffix}"),
            true,
        )
        .await;
        let inactive_series_id = insert_series(
            &pool,
            &format!("Inactive Trade Test {suffix}"),
            &format!("inactive-trade-test-{suffix}"),
            false,
        )
        .await;

        let offer_issue_id = insert_issue(&pool, active_series_id, 1, "Frozen Offer").await;
        let owned_issue_id = insert_issue(&pool, active_series_id, 2, "Already Owned").await;
        let wanted_issue_id = insert_issue(&pool, active_series_id, 3, "Already Wanted").await;
        let missing_issue_id = insert_issue(&pool, active_series_id, 4, "Missing Target").await;
        let concurrent_issue_id =
            insert_issue(&pool, active_series_id, 5, "Concurrent Target").await;
        let inactive_issue_id = insert_issue(&pool, inactive_series_id, 1, "Inactive Target").await;

        let author_id: u32 = sqlx::query("INSERT INTO persons (name) VALUES (?)")
            .bind(format!("Author {suffix}"))
            .execute(&pool)
            .await
            .expect("author fixture must be inserted")
            .last_insert_id()
            .try_into()
            .expect("author fixture ID must fit into u32");
        sqlx::query(
            "INSERT INTO issue_persons (issue_id, person_id, role) VALUES (?, ?, 'author')",
        )
        .bind(offer_issue_id)
        .bind(author_id)
        .execute(&pool)
        .await
        .expect("author relation fixture must be inserted");

        let offer_entry_id = insert_collection_entry(
            &pool,
            first_user_id,
            offer_issue_id,
            "duplicate",
            Some("Z1"),
        )
        .await;
        let owned_entry_id =
            insert_collection_entry(&pool, first_user_id, owned_issue_id, "owned", Some("Z2"))
                .await;
        let wanted_entry_id =
            insert_collection_entry(&pool, first_user_id, wanted_issue_id, "wanted", None).await;
        let foreign_offer_id = insert_collection_entry(
            &pool,
            second_user_id,
            missing_issue_id,
            "duplicate",
            Some("Z3"),
        )
        .await;

        let params = TradeListQueryParams {
            page: 1,
            per_page: 50,
            ..TradeListQueryParams::default()
        };
        let offers = find_trade_list_entries(&pool, first_user_id, "duplicate", &params)
            .await
            .expect("offers must load");
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].entry_id, offer_entry_id);
        assert_eq!(offers[0].owner_id, first_user_id);
        assert_eq!(offers[0].condition_grade.as_deref(), Some("Z1"));
        assert_ne!(offers[0].entry_id, foreign_offer_id);
        assert_eq!(
            count_trade_list_entries(&pool, first_user_id, "duplicate", &params)
                .await
                .expect("offers must count"),
            1
        );

        let author_search = TradeListQueryParams {
            q: Some(format!("Author {suffix}")),
            page: 1,
            per_page: 50,
            ..TradeListQueryParams::default()
        };
        assert_eq!(
            find_trade_list_entries(&pool, first_user_id, "duplicate", &author_search)
                .await
                .expect("offers must be searchable by author")
                .len(),
            1
        );

        let candidate_params = TradeListQueryParams {
            series_slug: Some(format!("trade-test-{suffix}")),
            page: 1,
            per_page: 50,
            ..TradeListQueryParams::default()
        };
        let candidates = find_wanted_candidates(&pool, first_user_id, &candidate_params)
            .await
            .expect("wanted candidates must load");
        assert_eq!(candidates.len(), 3);
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.issue_id != offer_issue_id
                    && candidate.issue_id != owned_issue_id)
        );
        assert_eq!(
            candidates
                .iter()
                .find(|candidate| candidate.issue_id == wanted_issue_id)
                .and_then(|candidate| candidate.wanted_entry_id),
            Some(wanted_entry_id)
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.issue_id == missing_issue_id)
        );
        assert_eq!(
            count_wanted_candidates(&pool, first_user_id, &candidate_params)
                .await
                .expect("wanted candidates must count"),
            3
        );

        let mutation = add_wanted_entries(
            &pool,
            first_user_id,
            &[
                owned_issue_id,
                wanted_issue_id,
                missing_issue_id,
                inactive_issue_id,
                u32::MAX,
            ],
        )
        .await
        .expect("bulk wanted mutation must succeed");
        assert_eq!(mutation.created.len(), 1);
        assert_eq!(mutation.created[0].issue_id, missing_issue_id);
        assert_eq!(mutation.unchanged.len(), 1);
        assert_eq!(mutation.unchanged[0].entry_id, wanted_entry_id);
        assert_eq!(mutation.rejected.len(), 3);
        assert!(mutation.rejected.iter().any(|rejection| {
            rejection.issue_id == owned_issue_id && rejection.reason == "already_owned"
        }));
        assert!(mutation.rejected.iter().any(|rejection| {
            rejection.issue_id == inactive_issue_id && rejection.reason == "issue_not_found"
        }));
        assert!(mutation.rejected.iter().any(|rejection| {
            rejection.issue_id == u32::MAX && rejection.reason == "issue_not_found"
        }));

        let stored_condition = sqlx::query_scalar::<_, Option<String>>(
            "SELECT condition_grade FROM collection_entries WHERE id = ?",
        )
        .bind(mutation.created[0].entry_id)
        .fetch_one(&pool)
        .await
        .expect("created wanted entry must exist");
        assert!(stored_condition.is_none());

        let repeated = add_wanted_entries(&pool, first_user_id, &[missing_issue_id])
            .await
            .expect("repeated wanted mutation must succeed");
        assert!(repeated.created.is_empty());
        assert_eq!(repeated.unchanged.len(), 1);
        assert_eq!(repeated.unchanged[0].entry_id, mutation.created[0].entry_id);

        let concurrent_ids = [concurrent_issue_id];
        let (first_concurrent, second_concurrent) = tokio::join!(
            add_wanted_entries(&pool, first_user_id, &concurrent_ids),
            add_wanted_entries(&pool, first_user_id, &concurrent_ids)
        );
        let first_concurrent = first_concurrent.expect("first concurrent mutation must succeed");
        let second_concurrent = second_concurrent.expect("second concurrent mutation must succeed");
        assert_eq!(
            first_concurrent.created.len() + second_concurrent.created.len(),
            1
        );
        assert_eq!(
            first_concurrent.unchanged.len() + second_concurrent.unchanged.len(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM collection_entries
                 WHERE user_id = ? AND issue_id = ? AND status = 'wanted'",
            )
            .bind(first_user_id)
            .bind(concurrent_issue_id)
            .fetch_one(&pool)
            .await
            .expect("concurrent wanted entries must count"),
            1
        );

        let foreign_wanted_id =
            insert_collection_entry(&pool, second_user_id, concurrent_issue_id, "wanted", None)
                .await;
        assert!(
            !delete_wanted_entry(&pool, first_user_id, foreign_wanted_id)
                .await
                .expect("foreign delete must be handled")
        );
        assert!(
            !delete_wanted_entry(&pool, first_user_id, owned_entry_id)
                .await
                .expect("owned delete must be handled")
        );
        assert!(
            delete_wanted_entry(&pool, first_user_id, wanted_entry_id)
                .await
                .expect("own wanted delete must succeed")
        );

        sqlx::query("DELETE FROM users WHERE id IN (?, ?)")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&pool)
            .await
            .expect("user fixtures must be deleted");
        sqlx::query("DELETE FROM series WHERE id IN (?, ?)")
            .bind(active_series_id)
            .bind(inactive_series_id)
            .execute(&pool)
            .await
            .expect("series fixtures must be deleted");
    }

    async fn insert_user(pool: &MySqlPool, email: &str, display_name: &str) -> u32 {
        sqlx::query("INSERT INTO users (email, display_name) VALUES (?, ?)")
            .bind(email)
            .bind(display_name)
            .execute(pool)
            .await
            .expect("user fixture must be inserted")
            .last_insert_id()
            .try_into()
            .expect("user fixture ID must fit into u32")
    }

    async fn insert_series(pool: &MySqlPool, name: &str, slug: &str, active: bool) -> u32 {
        sqlx::query("INSERT INTO series (name, slug, active) VALUES (?, ?, ?)")
            .bind(name)
            .bind(slug)
            .bind(active)
            .execute(pool)
            .await
            .expect("series fixture must be inserted")
            .last_insert_id()
            .try_into()
            .expect("series fixture ID must fit into u32")
    }

    async fn insert_issue(pool: &MySqlPool, series_id: u32, number: u32, title: &str) -> u32 {
        sqlx::query("INSERT INTO issues (series_id, issue_number, title) VALUES (?, ?, ?)")
            .bind(series_id)
            .bind(number)
            .bind(title)
            .execute(pool)
            .await
            .expect("issue fixture must be inserted")
            .last_insert_id()
            .try_into()
            .expect("issue fixture ID must fit into u32")
    }

    async fn insert_collection_entry(
        pool: &MySqlPool,
        user_id: u32,
        issue_id: u32,
        status: &str,
        condition: Option<&str>,
    ) -> u32 {
        sqlx::query(
            "INSERT INTO collection_entries
                (user_id, issue_id, copy_number, condition_grade, status)
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(user_id)
        .bind(issue_id)
        .bind(condition)
        .bind(status)
        .execute(pool)
        .await
        .expect("collection fixture must be inserted")
        .last_insert_id()
        .try_into()
        .expect("collection fixture ID must fit into u32")
    }
}
