use crate::app_state::AppState;
use axum::extract::Query;
use axum::Extension;
use axum::{extract::State, http::StatusCode, Json};
use db::{Project, Simulation, User};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct SimulationsRequest {
    wallet_address: Option<String>,
    project_slug: Option<String>,
}

#[derive(Serialize)]
pub struct StatsResponse {
    failure_simulations: i64,
    total_simulations: i64,
    unique_wallet_count: i64,
}

#[derive(Serialize)]
pub struct SimulationsResponse {
    simulations: Vec<Simulation>,
    stats: StatsResponse,
    project: Project,
}

pub async fn get_simulations(
    Extension(projects): Extension<Vec<Project>>,
    Extension(user): Extension<User>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<SimulationsRequest>,
) -> Result<Json<Option<SimulationsResponse>>, StatusCode> {
    let project = if let Some(project_slug) = query.project_slug.clone() {
        let project = projects.iter().find(|p| p.slug == project_slug).cloned();
        match project {
            Some(project) => Some(project),
            None => {
                if user.email.ends_with("@walnut.dev") {
                    let project_slug = query.project_slug.clone().unwrap_or_default();
                    match sqlx::query!("SELECT * FROM projects WHERE slug = $1", project_slug)
                        .fetch_one(&state.db_pool)
                        .await
                    {
                        Ok(row) => {
                            let project = Some(Project {
                                id: row.id,
                                name: row.name,
                                slug: row.slug,
                                // unpack other fields here
                            });
                            project
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
        }
    } else {
        projects.first().cloned()
    };

    if project.is_none() {
        return Ok(Json(None));
    }

    let project = project.unwrap().clone();
    let simulations = match query.project_slug {
        Some(project_slug) => {
            match sqlx::query_as!(
                Simulation,
                "SELECT simulations.*
                FROM simulations
                JOIN projects ON simulations.project_id = projects.id
                WHERE projects.slug = $1
                ORDER BY simulations.created_at DESC;",
                project_slug,
            )
            .fetch_all(&state.db_pool)
            .await
            {
                Ok(rows) => rows,
                Err(_) => Vec::new(),
            }
        }
        None => {
            match sqlx::query_as!(Simulation,
                "SELECT simulations.* FROM simulations WHERE project_id = $1 ORDER BY created_at DESC;",
                project.id
            )
            .fetch_all(&state.db_pool)
            .await
            {
                Ok(rows) => rows,
                Err(_) => Vec::new(),
            }
        }
    };

    let (simulations_with_failure_count, total_simulations_count, unique_wallet_count) =
        match sqlx::query!(
            r#"
            SELECT
                SUM(CASE WHEN status = 'failure' THEN 1 ELSE 0 END) as failure_count,
                COUNT(*) as total_count,
                COUNT(DISTINCT wallet_address) as unique_wallet_count
            FROM simulations
            WHERE project_id = $1 AND created_at >= NOW() - INTERVAL '7 days'
            "#,
            project.id
        )
        .fetch_one(&state.db_pool)
        .await
        {
            Ok(row) => (
                row.failure_count.unwrap_or(0),
                row.total_count.unwrap_or(0),
                row.unique_wallet_count.unwrap_or(0),
            ),
            Err(_) => (0, 0, 0),
        };

    Ok(Json(Some(SimulationsResponse {
        simulations,
        stats: StatsResponse {
            failure_simulations: simulations_with_failure_count,
            total_simulations: total_simulations_count,
            unique_wallet_count,
        },
        project: project.clone(),
    })))
}
