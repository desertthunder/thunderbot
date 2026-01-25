use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected_token = std::env::var("DASHBOARD_TOKEN").unwrap_or_else(|_| "changeme".to_string());
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match auth_header {
        Some(token) if token == expected_token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
