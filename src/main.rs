use clap::{Arg, ArgMatches, Command};
use hyper::service::service_fn;
use hyper::{Body, Request, Response, Server, StatusCode};
use percent_encoding::percent_decode;
use std::convert::Infallible;
use std::fs::{read, read_dir};
use std::net::SocketAddr;
use std::path::Path;
use std::process::exit;
use tera::{Context, Tera};
use tower::make::Shared;

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() {
    let args = get_args();
    let tera = Tera::new("templates/*.html").expect("templates should compile");

    let arg_port = args.value_of("port").unwrap();
    let port: u16 = arg_port.parse().unwrap_or_else(|_err| {
        eprintln!("Invalid port: {}", arg_port);
        exit(1);
    });

    let arg_directory = args.value_of("directory").unwrap();
    let directory = arg_directory.to_owned();

    // Create a service that calls my_server and passes in the directory
    let service = service_fn(move |request| {
        let tera = tera.clone();
        let directory = directory.clone();
        async { my_server(request, directory, tera).await }
    });
    let make_svc = Shared::new(service);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let server = Server::try_bind(&addr).unwrap_or_else(|err| {
        eprintln!("{}", err);
        exit(1);
    });

    let server = server.serve(make_svc);

    println!(
        "Serving HTTP on {0} port {1} (http://{0}:{1}/) ...",
        addr.ip(),
        addr.port()
    );

    if let Err(err) = server.await {
        eprintln!("server error: {err}");
    }
}

fn get_args() -> ArgMatches {
    Command::new("http_server")
        .version("0.1")
        .author("Collin Styles <collingstyles@gmail.com")
        .about("A port of Python3's http.server to Rust")
        .arg(
            Arg::new("port")
                .default_value("8000")
                .required(false)
                .help("Port to listen on"),
        )
        .arg(
            Arg::new("directory")
                .short('d')
                .long("directory")
                .default_value(".")
                .help("Port to listen on"),
        )
        .get_matches()
}

async fn my_server(
    req: Request<Body>,
    directory: String,
    tera: Tera,
) -> Result<Response<Body>, Infallible> {
    // TODO: Handle method (i.e., return error for POSTs, etc.)
    let uri_path = percent_decode(req.uri().path().as_bytes()).decode_utf8_lossy();
    let fs_path_string = format!("{}{}", directory, uri_path);
    let fs_path = Path::new(fs_path_string.as_str());

    if fs_path.is_dir() {
        if uri_path.ends_with('/') {
            // List the contents of the directory
            println!("path: {} || listing directory", uri_path);
            Ok(list_directory(&tera, fs_path, &uri_path))
        } else {
            // Redirect to the same directory but with a trailing /
            let new_path_string = format!("{}/", uri_path);
            println!("path: {} || redirecting to {}", uri_path, new_path_string);
            Ok(Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header("Location", new_path_string)
                .body(Body::empty())
                .unwrap())
        }
    } else if fs_path.is_file() {
        // Return the file object
        println!("path: {} || reading file", uri_path);
        Ok(read_file(fs_path))
    } else {
        // Doesn't exist
        eprintln!("Error: File/dir ({}) doesn't exist", uri_path);

        let mut context = Context::new();
        context.insert("error_code", "404");
        context.insert("message", "File not found");

        let result = match render(&tera, "error.html", &context) {
            Ok(body) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(body)
                .unwrap(),
            Err(resp) => resp,
        };

        Ok(result)
    }
}

fn render(tera: &Tera, template_file: &str, context: &Context) -> Result<Body, Response<Body>> {
    match tera.render(template_file, context) {
        Ok(body) => Ok(Body::from(body)),
        Err(error) => {
            let error = format!("Templating error: {}", error);
            eprintln!("{}", error);
            Err(Response::builder()
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
            Response::builder().body(Body::from(contents)).unwrap()
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            Response::builder()
                .body(Body::from(format!("Error: {}", e)))
                .unwrap()
        }
    }
}

fn list_directory(tera: &Tera, fs_path: &Path, uri_path: &str) -> Response<Body> {
    match read_dir(fs_path) {
        Ok(entries) => {
            // Create a sorted list of file_names (Strings)
            let mut v = Vec::new();
            if uri_path != "/" {
                v.push(String::from("../"));
            }
            for entry in entries {
                match entry {
                    Ok(e) => {
                        let mut file_name = e.file_name().to_string_lossy().to_string();
                        let p = e.path();
                        if Path::new(&p).is_dir() {
                            file_name.push('/');
                        }
                        v.push(file_name);
                    }
                    Err(e) => eprintln!("Err with read_dir: {}", e),
                }
            }
            v.sort_unstable();

            // List all files + directories
            let mut context = Context::new();
            context.insert("uri_path", uri_path);
            context.insert("entries", &v);

            match render(tera, "listing.html", &context) {
                Ok(body) => Response::builder().body(body).unwrap(),
                Err(resp) => resp,
            }
        }
        Err(e) => {
            // Insufficient permissions, probably
            eprintln!("path: {} || Error: {}", uri_path, e);
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(format!("Error: {}", e)))
                .unwrap()
        }
    }
}
