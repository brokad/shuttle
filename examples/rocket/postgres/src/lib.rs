#[macro_use]
extern crate rocket;

use rocket::{response::status::BadRequest, serde::json::Json, Build, Rocket, State};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, PgPool};
use shuttle_service::Factory;

#[macro_use]
extern crate shuttle_service;

#[get("/<id>")]
async fn retrieve(id: i32, state: &State<MyState>) -> Result<Json<Todo>, BadRequest<String>> {
    let todo = sqlx::query_as("SELECT * FROM todos WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| BadRequest(Some(e.to_string())))?;

    Ok(Json(todo))
}

#[post("/", data = "<data>")]
async fn add(
    data: Json<TodoNew>,
    state: &State<MyState>,
) -> Result<Json<Todo>, BadRequest<String>> {
    let todo = sqlx::query_as("INSERT INTO todos(note) VALUES ($1) RETURNING id, note")
        .bind(&data.note)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| BadRequest(Some(e.to_string())))?;

    Ok(Json(todo))
}

struct MyState {
    pool: PgPool,
}

fn rocket() -> Rocket<Build> {
    rocket::build().mount("/todo", routes![retrieve, add])
}

async fn build_state(factory: &mut dyn Factory) -> Result<MyState, shuttle_service::Error> {
    let pool = factory.get_postgres_connection_pool().await?;

    pool.execute(include_str!("../schema.sql")).await?;

    let state = MyState { pool };

    Ok(state)
}

declare_service!(Rocket<Build>, rocket, build_state);

#[derive(Deserialize)]
struct TodoNew {
    pub note: String,
}

#[derive(Serialize, FromRow)]
struct Todo {
    pub id: i32,
    pub note: String,
}
