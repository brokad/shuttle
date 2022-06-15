use std::convert::Infallible;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tonic::{async_trait, Request as TonicRequest, Response as TonicResponse, Status};

use http_body::combinators::{BoxBody, UnsyncBoxBody};
use hyper::{Request, Response};

use tower::util::BoxCloneService;
use tower::{steer::Steer, Service, ServiceExt};

use axum::body::{Bytes, HttpBody};
use axum::Error;
use axum::Router;

pub mod user_service {
    tonic::include_proto!("user_service");
}

use user_service::user_service_server::{UserService, UserServiceServer};
use user_service::{
    AddWorkspaceRequest, AddWorkspaceResponse, RemoveWorkspaceRequest, RemoveWorkspaceResponse,
};

struct MyUserService;

#[async_trait]
impl UserService for MyUserService {
    async fn add_workspace_id(
        &self,
        _req: TonicRequest<AddWorkspaceRequest>,
    ) -> Result<TonicResponse<AddWorkspaceResponse>, Status> {
        todo!()
    }

    async fn remove_workspace_id(
        &self,
        _req: TonicRequest<RemoveWorkspaceRequest>,
    ) -> Result<TonicResponse<RemoveWorkspaceResponse>, Status> {
        todo!()
    }
}

struct MyAxumService;
#[derive(Clone)]
struct HelloWorld;

impl tower::Service<hyper::Request<hyper::Body>> for HelloWorld {
    type Response = hyper::Response<hyper::Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: hyper::Request<hyper::Body>) -> Self::Future {
        let body = hyper::Body::from("Hello, world!");
        let resp = hyper::Response::builder()
            .status(200)
            .body(body)
            .expect("Unable to create the `hyper::Response` object");

        let fut = async { Ok(resp) };

        Box::pin(fut)
    }
}

type SteerRequest = Request<BoxBody<Bytes, Error>>;

type SteerResponse = Response<UnsyncBoxBody<Bytes, Error>>;

type SteerService = BoxCloneService<SteerRequest, SteerResponse, Infallible>;

#[shuttle_service::main]
async fn tower() -> Result<SteerService, shuttle_service::Error> {
    let my_user_service: SteerService =
        ServiceExt::<SteerRequest>::boxed(UserServiceServer::new(MyUserService))
            .map_err(|_| unreachable!())
            .map_response(|resp| {
                let (parts, body) = resp.into_parts();
                let body = body
                    .map_err(|_status| todo!("handle errors"))
                    .boxed_unsync();
                Response::from_parts(parts, body)
            })
        .boxed_clone();

    let my_axum_router: SteerService = Router::<BoxBody<Bytes, Error>>::new()
        .boxed_clone();

    let service_head: Steer<SteerService, _, SteerRequest> = Steer::new(
        vec![my_axum_router],
        |_req: &SteerRequest, _services: &[SteerService]| todo!("steering logic"),
    );

    Ok(service_head.boxed_clone())
}
