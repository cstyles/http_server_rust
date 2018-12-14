use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use std::path::{Path};
use std::fs::{read_dir, read};
use percent_encoding::percent_decode;
use clap::{App, Arg, ArgMatches};
use tera::{Tera, Context, compile_templates};

lazy_static::lazy_static! {
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
    let uri_path = percent_decode(req.uri().path().as_bytes()).decode_utf8_lossy();
    let fs_path_string = format!("{}{}", directory, uri_path);
    let fs_path = Path::new(fs_path_string.as_str());

    if fs_path.is_dir() {
        if uri_path.ends_with("/") {
            // List the contents of the directory
            println!("path: {} || listing directory", uri_path);
            list_directory(fs_path, &uri_path)
        } else {
            // Redirect to the same directory but with a trailing /
            let new_path_string = format!("{}/", uri_path);
            println!("path: {} || redirecting to {}", uri_path, new_path_string);
            Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header("Location", new_path_string)
                .body(Body::empty())
                .unwrap()
        }
    } else if fs_path.is_file() {
        // Return the file object
        println!("path: {} || reading file", uri_path);
        read_file(fs_path)
    } else {
        // Doesn't exist
        eprintln!("Error: File/dir ({}) doesn't exist", uri_path);

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

fn read_file(fs_path: &Path) -> Response<Body> {
    let file_contents = read(fs_path);

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

fn list_directory(fs_path: &Path, uri_path: &str) -> Response<Body> {
    match read_dir(fs_path) {
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
            context.insert("uri_path", uri_path);
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
            eprintln!("path: {} || Error: {}", uri_path, e);
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(format!("Error: {}", e)))
                .unwrap()
        },
    }
}
