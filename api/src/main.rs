#[macro_use]
extern crate rocket;

#[macro_use]
extern crate log;

mod args;
mod auth;
mod auth_admin;
mod build;
mod database;
mod deployment;
mod factory;
mod proxy;
mod router;

use auth_admin::Admin;
use deployment::MAX_DEPLOYS;
use factory::ShuttleFactory;
use rocket::serde::json::Json;
use rocket::{tokio, Build, Data, Rocket, State};
use shuttle_common::project::ProjectConfig;
use shuttle_common::{DeploymentApiError, DeploymentMeta, Port};
use std::net::IpAddr;
use std::sync::Arc;
use structopt::StructOpt;
use uuid::Uuid;

use crate::args::Args;
use crate::auth::{ApiKey, AuthorizationError, User, USER_DIRECTORY};
use crate::build::{BuildSystem, FsBuildSystem};
use crate::deployment::DeploymentSystem;

type ApiResult<T, E> = Result<Json<T>, E>;

/// Find user by username and return it's API Key.
/// if user does not exist create it and update `users` state to `users.toml`.
/// Finally return user's API Key.
#[post("/users/<username>")]
async fn get_or_create_user(username: String, _admin: Admin) -> Result<ApiKey, AuthorizationError> {
    USER_DIRECTORY.get_or_create(username)
}

/// Status API to be used to check if the service is alive
#[get("/status")]
async fn status() {}

#[get("/deployments/<id>")]
async fn get_deployment(
    state: &State<ApiState>,
    id: Uuid,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    let deployment = state.deployment_manager.get_deployment(&id).await?;

    validate_user_for_deployment(&user, &deployment)?;

    Ok(Json(deployment))
}

#[delete("/deployments/<id>")]
async fn delete_deployment(
    state: &State<ApiState>,
    id: Uuid,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    let deployment = state.deployment_manager.get_deployment(&id).await?;

    validate_user_for_deployment(&user, &deployment)?;

    let deployment = state.deployment_manager.kill_deployment(&id).await?;

    Ok(Json(deployment))
}

#[get("/projects/<project_name>")]
async fn get_project(
    state: &State<ApiState>,
    project_name: String,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    validate_user_for_project(&user, &project_name)?;

    let deployment = state
        .deployment_manager
        .get_deployment_for_project(&project_name)
        .await?;

    Ok(Json(deployment))
}

#[delete("/projects/<project_name>")]
async fn delete_project(
    state: &State<ApiState>,
    project_name: String,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    validate_user_for_project(&user, &project_name)?;

    let deployment = state
        .deployment_manager
        .kill_deployment_for_project(&project_name)
        .await?;
    Ok(Json(deployment))
}

#[post("/projects", data = "<crate_file>")]
async fn create_project(
    state: &State<ApiState>,
    crate_file: Data<'_>,
    project: ProjectConfig,
    user: User,
) -> ApiResult<DeploymentMeta, DeploymentApiError> {
    USER_DIRECTORY.validate_or_create_project(&user, project.name())?;

    let deployment = state
        .deployment_manager
        .deploy(crate_file, &project)
        .await?;
    Ok(Json(deployment))
}

fn validate_user_for_project(user: &User, project_name: &String) -> Result<(), DeploymentApiError> {
    if !user.projects.contains(project_name) {
        log::warn!(
            "failed to authenticate user {:?} for project `{}`",
            &user,
            project_name
        );
        Err(DeploymentApiError::NotFound(format!(
            "could not find project `{}`",
            &project_name
        )))
    } else {
        Ok(())
    }
}

fn validate_user_for_deployment(
    user: &User,
    meta: &DeploymentMeta,
) -> Result<(), DeploymentApiError> {
    if !user.projects.contains(meta.config.name()) {
        log::warn!(
            "failed to authenticate user {:?} for deployment `{}`",
            &user,
            &meta.id
        );
        Err(DeploymentApiError::NotFound(format!(
            "could not find deployment `{}`",
            &meta.id
        )))
    } else {
        Ok(())
    }
}

struct ApiState {
    deployment_manager: Arc<DeploymentSystem>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(MAX_DEPLOYS)
        .build()
        .unwrap()
        .block_on(async {
            rocket().await.launch().await?;

            Ok(())
        })
}

//noinspection ALL
async fn rocket() -> Rocket<Build> {
    env_logger::Builder::new()
        .filter_module("rocket", log::LevelFilter::Warn)
        .filter_module("_", log::LevelFilter::Warn)
        .filter_module("api", log::LevelFilter::Debug)
        .init();

    let args: Args = Args::from_args();
    let build_system = FsBuildSystem::initialise(args.path).unwrap();
    let deployment_manager = Arc::new(DeploymentSystem::new(Box::new(build_system)).await);

    start_proxy(args.bind_addr, args.proxy_port, deployment_manager.clone()).await;

    let state = ApiState { deployment_manager };

    let config = rocket::Config {
        address: args.bind_addr,
        port: args.api_port,
        ..Default::default()
    };
    rocket::custom(config)
        .mount(
            "/",
            routes![
                delete_deployment,
                get_deployment,
                delete_project,
                create_project,
                get_project,
                get_or_create_user,
                status
            ],
        )
        .manage(state)
}

async fn start_proxy(
    bind_addr: IpAddr,
    proxy_port: Port,
    deployment_manager: Arc<DeploymentSystem>,
) {
    tokio::spawn(async move { proxy::start(bind_addr, proxy_port, deployment_manager).await });
}
