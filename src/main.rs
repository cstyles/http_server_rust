extern crate hyper;

#[macro_use]
extern crate tera;
#[macro_use]
extern crate lazy_static;

use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use tera::{Tera, Context};

use std::path::{Path};
use std::fs::{read_dir, read};

lazy_static! {
    pub static ref TERA: Tera = compile_templates!("templates/*.html");
}

fn main() {
    // let addr = ([127, 0, 0, 1], 8000).into();
    let addr = ([0, 0, 0, 0], 8000).into();
    let new_svc = || {
        service_fn_ok(my_server)
    };
    let server = Server::bind(&addr)
        .serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Serving HTTP on {0} port {1} (http://{0}:{1}/) ...",
        addr.ip(),
        addr.port());

    hyper::rt::run(server);
}

fn my_server(req: Request<Body>) -> Response<Body> {
    let path_str = req.uri().path();
    let local_path_string = format!(".{}", path_str);
    let path = Path::new(local_path_string.as_str());

    if path.is_dir() {
        if path.to_str().unwrap().ends_with("/") {
            // List the contents of the directory
            println!("path: {} || listing directory", path_str);
            list_directory(path, &path_str)
        } else {
            // Redirect to the same directory but with a trailing /
            let new_path_string = format!("{}/", path_str);
            println!("path: {} || redirecting to {}", path_str, new_path_string);
            Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header("Location", new_path_string)
                .body(Body::empty())
                .unwrap()
        }
    } else if path.is_file() {
        // Return the file object
        println!("path: {:?} || reading file", path);
        read_file(path)
    } else {
        // Doesn't exist
        eprintln!("Error: File/dir doesn't exist");
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Error: File/dir doesn't exist"))
            .unwrap()
    }
}

fn read_file(path: &Path) -> Response<Body> {
    let file_contents = read(path);

    match file_contents {
        Ok(contents) => {
            // TODO: headers (MIME type, etc.)
            Response::builder()
                .body(Body::from(contents))
                .unwrap()
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            Response::builder()
                .body(Body::from(format!("Error: {}", e)))
                .unwrap()
        },
    }
}

fn list_directory(path: &Path, path_str: &str) -> Response<Body> {
    match read_dir(path) {
        Ok(entries) => {
            // Create a sorted list of file_names (Strings)
            let mut v = Vec::new();
            for entry in entries {
                match entry {
                    Ok(e) => {
                        let file_name = e.file_name().into_string().unwrap();
                        // TODO: Add '/' if directory
                        v.push(file_name);
                    },
                    Err(e) => eprintln!("Err with read_dir: {}", e),
                }
            }
            v.sort_unstable();

            // List all files + directories
            // TODO:
            // - Prettify, add links
            // - Unicode
            let mut context = Context::new();
            context.insert("path_str", path_str);
            context.insert("entries", &v);
            let body = TERA.render("listing.html", &context).unwrap();
            Response::builder()
                .body(Body::from(body))
                .unwrap()
        },
        Err(e) => {
            // Insufficient permissions, probably
            eprintln!("path: {:?} || Error: {}", path, e);
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(format!("Error: {}", e)))
                .unwrap()
        },
    }
}
