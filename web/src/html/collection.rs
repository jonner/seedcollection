use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    format_id_number,
    state::AppState,
    Message, MessageType, TemplateKey,
};
use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    filter::{Cmp, FilterBuilder, FilterOp},
    loadable::Loadable,
    project::{self, Project},
    sample::{self, Sample},
    source,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Row};
use std::sync::Arc;
use tracing::warn;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(show_new_project).post(insert_project))
        .route("/list", get(list_projects))
        .route("/filter", get(filter_projects))
        .route(
            "/:id",
            get(show_project).put(modify_project).delete(delete_project),
        )
        .route("/:id/edit", get(show_project))
        .route("/:id/filter", get(filter_project_samples))
        .route("/:id/add", get(show_add_sample).post(add_sample))
        .nest("/:id/sample/", super::allocation::router())
}

async fn list_projects(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let projects = Project::fetch_all(
        Some(Arc::new(project::Filter::User(user.id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collections => projects),
    )
    .into_response())
}

#[derive(Deserialize)]
struct FilterParams {
    fragment: String,
}

async fn filter_projects(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Result<impl IntoResponse, error::Error> {
    let namefilter = FilterBuilder::new(FilterOp::Or)
        .push(Arc::new(project::Filter::Name(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .push(Arc::new(project::Filter::Description(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .build();
    let filter = FilterBuilder::new(FilterOp::And)
        .push(namefilter)
        .push(Arc::new(project::Filter::User(user.id)))
        .build();

    let projects = Project::fetch_all(Some(filter), &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collections => projects),
    )
    .into_response())
}

async fn show_new_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
}

#[derive(Deserialize, Serialize)]
struct ProjectParams {
    name: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    description: Option<String>,
}

async fn do_insert(
    user: SqliteUser,
    params: &ProjectParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }

    let mut project = Project::new(
        params.name.clone(),
        params.description.as_ref().cloned(),
        user.id,
    );
    project.insert(&state.dbpool).await.map_err(|e| e.into())
}

async fn insert_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, error::Error> {
    match do_insert(user, &params, &state).await {
        Err(e) => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context!( message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to save collection: {}", e),
                },
                request => params),
        )
        .into_response()),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let projecturl = app_url(&format!("/collection/{}", id));

            Ok((
                [("HX-Redirect", projecturl)],
                RenderHtml(
                    key,
                    state.tmpl.clone(),
                    context!( message =>
                    Message {
                        r#type: MessageType::Success,
                        msg: format!(
                            r#"Added new collection {}: {} to the database"#,
                            id, params.name
                            )
                    },
                    ),
                ),
            )
                .into_response())
        }
    }
}

async fn show_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = FilterBuilder::new(FilterOp::And)
        .push(Arc::new(project::Filter::Id(id)))
        .push(Arc::new(project::Filter::User(user.id)));
    let mut projects = Project::fetch_all(Some(fb.build()), &state.dbpool).await?;
    let Some(mut project) = projects.pop() else {
        return Err(Error::NotFound(
            "That collection does not exist".to_string(),
        ));
    };
    project.fetch_samples(None, &state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => project),
    )
    .into_response())
}

async fn do_update(
    id: i64,
    params: &ProjectParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    let mut project = Project::fetch(id, &state.dbpool).await?;
    project.name = params.name.clone();
    project.description = params.description.clone();
    project.update(&state.dbpool).await.map_err(|e| e.into())
}

async fn modify_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<ProjectParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = FilterBuilder::new(FilterOp::And)
        .push(Arc::new(project::Filter::Id(id)))
        .push(Arc::new(project::Filter::User(user.id)));
    let projects = Project::fetch_all(Some(fb.build()), &state.dbpool).await?;
    if projects.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let (request, message, headers) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
            },
            None,
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: "Successfully updated collection".to_string(),
            },
            Some([("HX-Redirect", app_url(&format!("/collection/{id}")))]),
        ),
    };
    let project = Project::fetch(id, &state.dbpool).await?;
    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(collection => project,
             message => message,
             request => request,
            ),
        ),
    )
        .into_response())
}

async fn delete_project(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut project = Project::fetch(id, &state.dbpool)
        .await
        .map_err(|_| Error::NotFound("That collection does not exist".to_string()))?;
    if project.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this collection".to_string(),
        ));
    }

    let errmsg = match project.delete(&state.dbpool).await {
        Err(e) => e.to_string(),
        Ok(res) if (res.rows_affected() == 0) => "No collection found".to_string(),
        Ok(_) => {
            return Ok((
                [("HX-Redirect", app_url("/collection/list"))],
                RenderHtml(key, state.tmpl.clone(), context!(deleted => true, id => id)),
            )
                .into_response())
        }
    };
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(
        collection => project,
        message => Message {
            r#type: MessageType::Error,
            msg: format!("Failed to delete collection: {errmsg}")
        },
        ),
    )
    .into_response())
}

async fn add_sample_prep(
    user: &SqliteUser,
    id: i64,
    state: &AppState,
) -> Result<(Project, Vec<Sample>), error::Error> {
    let project = Project::fetch(id, &state.dbpool).await?;

    let ids_in_project = sqlx::query!(
        "SELECT CS.sampleid from sc_collection_samples CS WHERE CS.collectionid=?",
        id
    )
    .fetch_all(&state.dbpool)
    .await?;
    let ids = ids_in_project.iter().map(|row| row.sampleid).collect();

    /* FIXME: make this more efficient by changeing it to filter by
     *  'WHERE NOT IN (SELECT ids from...)'
     * instead of
     * [ query ids first ], then
     *  'WHERE NOT IN (1, 2, 3, 4, 5...)'
     */
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Arc::new(sample::Filter::SampleNotIn(ids))),
        &state.dbpool,
    )
    .await?;
    Ok((project, samples))
}

async fn show_add_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let (c, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => c,
                 samples => samples),
    )
    .into_response())
}

async fn add_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, error::Error> {
    let toadd: Vec<i64> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<i64>().ok(),
            _ => None,
        })
        .collect();
    let res = sqlx::query!("SELECT userid FROM sc_collections WHERE collectionid=?", id)
        .fetch_one(&state.dbpool)
        .await?;
    if res.userid != Some(user.id) {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let mut qb =
        sqlx::QueryBuilder::new("SELECT sampleid, userid FROM sc_samples WHERE sampleid IN (");
    let mut sep = qb.separated(", ");
    for id in toadd {
        sep.push_bind(id);
    }
    qb.push(")");
    let res = qb.build().fetch_all(&state.dbpool).await?;
    let valid_samples = res.iter().filter_map(|row| {
        let userid: Option<i64> = row.try_get("userid").ok()?;
        let userid = userid?;
        let id: i64 = row.try_get("sampleid").ok()?;
        if userid == user.id {
            Some(id)
        } else {
            warn!(
                "dropping sample {} which is not owned by user {}",
                id, user.id
            );
            None
        }
    });

    let mut project = Project::fetch(id, &state.dbpool).await?;
    let mut n_inserted = 0;
    let mut messages = Vec::new();
    for sample in valid_samples {
        let s = Sample::new_loadable(sample);
        match project.allocate_sample(s, &state.dbpool).await {
            Err(e) => messages.push(Message {
                r#type: MessageType::Error,
                msg: format!(
                    "Failed to add sample {}: {}",
                    format_id_number(sample, Some("S"), None),
                    e.to_string()
                ),
            }),
            Ok(res) => n_inserted += res.rows_affected(),
        }
    }

    if n_inserted > 0 {
        messages.insert(
            0,
            Message {
                r#type: MessageType::Success,
                msg: format!("Assigned {n_inserted} samples to this collection"),
            },
        );
    }

    let (project, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(collection => project,
                 messages => messages,
                 samples => samples),
    )
    .into_response())
}

async fn filter_project_samples(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<FilterParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fragment = params.fragment.trim();
    let mut fbuilder =
        FilterBuilder::new(FilterOp::And).push(Arc::new(sample::Filter::User(user.id)));
    if !fragment.is_empty() {
        let subfilter = FilterBuilder::new(FilterOp::Or)
            .push(Arc::new(sample::Filter::TaxonNameLike(
                params.fragment.clone(),
            )))
            .push(Arc::new(source::Filter::Name(
                Cmp::Like,
                params.fragment.clone(),
            )))
            .push(Arc::new(sample::Filter::Notes(
                Cmp::Like,
                params.fragment.clone(),
            )))
            .build();
        fbuilder = fbuilder.push(subfilter);
    }

    let mut project = Project::fetch(id, &state.dbpool).await?;
    project
        .fetch_samples(Some(fbuilder.build()), &state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => project),
    )
    .into_response())
}
