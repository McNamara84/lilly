use serde_json::json;
use sqlx::MySqlPool;

use crate::db::{messaging, notifications};
use crate::error::AppError;
use crate::models::messaging::{
    MessageListResponse, MessagePageParams, MessageResponse, PaginatedThreadsResponse,
    SendMessageRequest, ThreadPageParams, ThreadSummaryResponse,
};

pub async fn list_threads(
    pool: &MySqlPool,
    user_id: u32,
    params: &ThreadPageParams,
) -> Result<PaginatedThreadsResponse, AppError> {
    let total = messaging::count_threads(pool, user_id).await?;
    let data = messaging::find_threads(pool, user_id, params)
        .await?
        .iter()
        .map(ThreadSummaryResponse::from)
        .collect();
    Ok(PaginatedThreadsResponse {
        data,
        page: params.page(),
        per_page: params.per_page(),
        total,
    })
}

pub async fn list_messages(
    pool: &MySqlPool,
    user_id: u32,
    thread_id: u32,
    params: &MessagePageParams,
) -> Result<MessageListResponse, AppError> {
    messaging::find_thread_access(pool, thread_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    let mut rows = messaging::find_messages(pool, thread_id, params).await?;
    let next_before_id = (rows.len() == params.limit() as usize)
        .then(|| rows.last().map(|message| message.id))
        .flatten();
    rows.reverse();
    Ok(MessageListResponse {
        data: rows
            .into_iter()
            .map(|message| MessageResponse::from_record(message, user_id))
            .collect(),
        next_before_id,
    })
}

pub async fn send_message(
    pool: &MySqlPool,
    user_id: u32,
    thread_id: u32,
    request: &SendMessageRequest,
) -> Result<(MessageResponse, bool), AppError> {
    let content = request.validate().map_err(AppError::BadRequest)?;
    let mut transaction = pool.begin().await?;
    let access = messaging::lock_thread_access(&mut transaction, thread_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    if !matches!(access.trade_status.as_str(), "proposed" | "accepted") {
        return Err(AppError::ConflictWithCode {
            message: "Messages can only be sent while a trade is open".to_string(),
            code: "trade_closed".to_string(),
        });
    }
    let recipient_id = access
        .recipient_id(user_id)
        .ok_or_else(resource_not_found)?;
    let (message, created) = match messaging::insert_message(
        &mut transaction,
        thread_id,
        user_id,
        &request.client_message_id,
        content,
    )
    .await
    {
        Ok(message_id) => {
            let message = messaging::find_message_by_id(&mut transaction, message_id).await?;
            (message, true)
        }
        Err(error)
            if matches!(
                &error,
                sqlx::Error::Database(database_error)
                    if database_error.kind() == sqlx::error::ErrorKind::UniqueViolation
            ) =>
        {
            let existing = messaging::find_message_by_client_id(
                &mut transaction,
                thread_id,
                user_id,
                &request.client_message_id,
            )
            .await?
            .ok_or(error)?;
            (existing, false)
        }
        Err(error) => return Err(error.into()),
    };
    if created {
        notifications::insert_notification(
            &mut transaction,
            recipient_id,
            Some(user_id),
            "trade_message",
            None,
            Some(access.trade_id),
            Some(message.id),
            &format!("message:{}", message.id),
            &json!({
                "thread_id": thread_id,
                "trade_id": access.trade_id,
                "sender_id": user_id
            }),
        )
        .await?;
        messaging::touch_thread(&mut transaction, thread_id).await?;
    }
    transaction.commit().await?;
    Ok((MessageResponse::from_record(message, user_id), created))
}

pub async fn mark_thread_read(
    pool: &MySqlPool,
    user_id: u32,
    thread_id: u32,
    through_message_id: u32,
) -> Result<(), AppError> {
    if through_message_id == 0 {
        return Err(AppError::BadRequest(
            "through_message_id must be positive".to_string(),
        ));
    }
    let mut transaction = pool.begin().await?;
    messaging::lock_thread_access(&mut transaction, thread_id, user_id)
        .await?
        .ok_or_else(resource_not_found)?;
    messaging::mark_thread_read(&mut transaction, thread_id, user_id, through_message_id).await?;
    transaction.commit().await?;
    Ok(())
}

fn resource_not_found() -> AppError {
    AppError::NotFound("Resource not found".to_string())
}
