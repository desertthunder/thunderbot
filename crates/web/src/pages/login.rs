use crate::AppState;
use crate::partials;
use axum::Router;
use axum::extract::{Form, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use maud::{DOCTYPE, Markup, html};
use serde::Deserialize;

const SESSION_COOKIE: &str = "tnbot_session";

#[derive(Debug, Deserialize, Default)]
struct LoginQuery {
    error: Option<String>,
    notice: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginFormData {
    username: String,
    password: String,
}

#[derive(Debug, Default)]
struct LoginTemplateContext<'a> {
    error: Option<&'a str>,
    notice: Option<&'a str>,
}

impl<'a> From<&'a LoginQuery> for LoginTemplateContext<'a> {
    fn from(value: &'a LoginQuery) -> Self {
        Self { error: value.error.as_deref(), notice: value.notice.as_deref() }
    }
}

pub(crate) fn public_routes() -> Router<AppState> {
    Router::new().route("/login", get(login_page).post(login_submit))
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new().route("/logout", post(logout))
}

pub(crate) async fn require_auth(
    State(state): State<AppState>, jar: CookieJar, request: Request, next: Next,
) -> Response {
    if has_valid_session(&state, &jar).await {
        return next.run(request).await;
    }

    if request.headers().contains_key("HX-Request") {
        return (StatusCode::UNAUTHORIZED, [("HX-Redirect", "/login")]).into_response();
    }

    Redirect::to("/login").into_response()
}

async fn login_page(
    State(state): State<AppState>, jar: CookieJar, Query(query): Query<LoginQuery>,
) -> impl IntoResponse {
    if has_valid_session(&state, &jar).await {
        Redirect::to("/dashboard").into_response()
    } else {
        let context = LoginTemplateContext::from(&query);
        Html(login_view(&context).into_string()).into_response()
    }
}

async fn login_submit(
    State(state): State<AppState>, jar: CookieJar, Form(form): Form<LoginFormData>,
) -> impl IntoResponse {
    if !state
        .auth
        .verify_credentials(form.username.trim(), form.password.trim())
    {
        return Redirect::to("/login?error=Invalid%20credentials").into_response();
    }

    let token = state.auth.issue_session().await;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    (jar.add(cookie), Redirect::to("/dashboard")).into_response()
}

async fn logout(jar: CookieJar) -> impl IntoResponse {
    let cookie = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    (jar.remove(cookie), Redirect::to("/login?notice=Signed%20out")).into_response()
}

async fn has_valid_session(state: &AppState, jar: &CookieJar) -> bool {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        state.auth.validate_session(cookie.value()).await
    } else {
        false
    }
}

fn login_view(context: &LoginTemplateContext<'_>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            (partials::head("Thunderbot Login"))
            body class="auth" {
                main class="container" {
                    article {
                        header {
                            p class="brand" {
                                span class="brand-mark" { "T" }
                                strong { "Thunderbot Control Deck" }
                            }
                            p class="muted" {
                                "Sign in to access dashboard, chat inspector, and manual controls."
                            }
                        }

                        (partials::notices(context.notice, context.error))

                        form method="post" action="/login" {
                            label for="username" {
                                "Username"
                                input id="username" name="username" autocomplete="username" required;
                            }
                            label for="password" {
                                "Password"
                                input id="password" name="password" type="password" autocomplete="current-password" required;
                            }
                            button type="submit" { "Sign In" }
                        }
                    }
                }
            }
        }
    }
}
