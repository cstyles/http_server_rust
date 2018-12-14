extern crate hyper;

extern crate percent_encoding;
use percent_encoding::percent_decode;

extern crate clap;
use clap::{App, Arg, ArgMatches};

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
    let args = get_args();

    let arg_port = args.value_of("port").unwrap();
    let port: u16 = match arg_port.parse() {
        Ok(port) => port,
        Err(_) => {
            eprintln!("Invalid port: {}", arg_port);
            return
        }
    };

    let arg_directory = args.value_of("directory").unwrap();
    let directory = arg_directory.to_owned();

    // Create a service that calls my_server and passes in the directory
    let new_svc = move || {
        // This clone is necessary so that "directory" (a String) isn't
        // referring to a borrowed str from the main method's context
        let directory = directory.clone();
        service_fn_ok(move |request| {
            my_server(request, &directory)
        })
    };

    let addr = ([0, 0, 0, 0], port).into();
    let server = match Server::try_bind(&addr) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("{}", e);
            return
        }
    };

    let server = server.serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Serving HTTP on {0} port {1} (http://{0}:{1}/) ...",
        addr.ip(),
        addr.port());

    hyper::rt::run(server);
}

fn get_args<'a>() -> ArgMatches<'a> {
    App::new("http_server")
        .version("0.1")
        .author("Collin Styles <collingstyles@gmail.com")
        .about("A port of Python3's http.server to Rust")
        .arg(Arg::with_name("port")
                 .default_value("8000")
                 .required(false)
                 .help("Port to listen on"))
        .arg(Arg::with_name("directory")
                 .short("d")
                 .long("directory")
                 .default_value(".")
                 .help("Port to listen on"))
        .get_matches()
}

fn my_server(req: Request<Body>, directory: &str) -> Response<Body> {
    // TODO: Handle method (i.e., return error for POSTs, etc.)
    let path_str = percent_decode(req.uri().path().as_bytes()).decode_utf8_lossy();
    let local_path_string = format!("{}{}", directory, path_str);
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
        eprintln!("Error: File/dir ({}) doesn't exist", path_str);

        let mut context = Context::new();
        context.insert("error_code", "404");
        context.insert("message", "File not found");

        match render("error.html", &context) {
            Ok(body) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(body)
                .unwrap(),
            Err(resp) => resp,
        }
    }
}

fn render(template_file: &str, context: &Context) -> Result<Body, Response<Body>> {
    match TERA.render(template_file, context) {
        Ok(body) => Ok(Body::from(body)),
        Err(error) => {
            let error = format!("Templating error: {}", error);
            eprintln!("{}", error);
            return Err(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(error))
                .unwrap())
        }
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
                        let mut file_name = e.file_name().into_string().unwrap();
                        let p = e.path();
                        if Path::new(&p).is_dir() {
                            file_name.push_str("/");
                        }
                        v.push(file_name);
                    },
                    Err(e) => eprintln!("Err with read_dir: {}", e),
                }
            }
            v.sort_unstable();

            // List all files + directories
            let mut context = Context::new();
            context.insert("path_str", path_str);
            context.insert("entries", &v);

            match render("listing.html", &context) {
                Ok(body) => Response::builder()
                    .body(body)
                    .unwrap(),
                Err(resp) => resp,
            }
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
