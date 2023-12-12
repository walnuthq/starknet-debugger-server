use crate::app_state::AppState;
use crate::db::{Project, User};
use axum::{
    extract::State, http::Method, http::Request, http::StatusCode, middleware::Next,
    response::Response,
};
use cookie::Cookie;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    email: String,
    sub: String,
    projects: Vec<String>,
    iat: i32,
}

pub async fn user_auth_middleware<B>(
    State(_state): State<Arc<AppState>>,
    mut req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    if req.method() != Method::OPTIONS {
        let session_token = if let Some(header_value) = req.headers().get("Authorization") {
            if let Ok(auth_str) = header_value.to_str() {
                if auth_str.starts_with("Bearer ") {
                    Some(auth_str.trim_start_matches("Bearer "))
                } else {
                    return Err(StatusCode::UNAUTHORIZED);
                }
            } else {
                return Err(StatusCode::UNAUTHORIZED);
            }
        } else if let Some(cookie_value) = req.headers().get("cookie") {
            // Parse cookies and look for "session-token"
            if let Ok(cookie_str) = cookie_value.to_str() {
                for cookie in Cookie::split_parse_encoded(cookie_str) {
                    if let Ok(cookie) = cookie {
                        if cookie.name() == "session-token" {
                            Some(cookie.value());
                        }
                    }
                }
            }
            return Err(StatusCode::UNAUTHORIZED);
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        };

        if let Some(token) = session_token {
            if let Ok(data) = decode::<Claims>(
                token,
                &DecodingKey::from_secret(
                    b"cc7e0d44fd473002f1c42167459001140ec6389b7353f8088f4d9a95f2f596f2",
                ),
                &Validation::new(Algorithm::HS256),
            ) {
                let claims = data.claims;
                let email = claims.email;

                // Check if user exists
                let user_query = (sqlx::query!(
                    r#"
                    SELECT * FROM users WHERE email = $1
                    "#,
                    email
                )
                .fetch_optional(&_state.db_pool)
                .await)
                    .unwrap();

                if let Some(user) = user_query {
                    let projects_query = (sqlx::query!(
                        r#"
                        SELECT projects.*
                        FROM projects
                        INNER JOIN users_projects ON projects.id = users_projects.project_id
                        WHERE users_projects.user_id = $1
                        "#,
                        user.id
                    )
                    .fetch_all(&_state.db_pool)
                    .await)
                        .unwrap();

                    let mut projects = Vec::new();
                    for row in projects_query {
                        projects.push(Project {
                            id: row.id,
                            name: row.name,
                        });
                    }
                    req.extensions_mut().insert(projects);
                } else {
                    sqlx::query!(
                        r#"
                        INSERT INTO users (email) VALUES ($1) RETURNING *
                        "#,
                        email
                    )
                    .fetch_one(&_state.db_pool)
                    .await;

                    req.extensions_mut().insert(User { email });

                    req.extensions_mut().insert(Vec::<Project>::new());
                }
            } else {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    Ok(next.run(req).await)
}
