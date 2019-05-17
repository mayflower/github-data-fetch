#[macro_use]
extern crate clap;
extern crate futures;
extern crate hubcaps;
extern crate hyper;
extern crate rmp_serde;
extern crate serde;
extern crate stream_throttle;
extern crate tokio_core;

use std::error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::Future;
use futures::future;
use futures::stream::Stream;
use hubcaps::{Credentials, Github, ErrorKind};
use hubcaps::repositories::Repository;
use hubcaps::issues::{Issue, IssueListOptions, State};
use hubcaps::pulls::Pull;
use hyper::client::connect::Connect;
use rmp_serde::Serializer;
use serde::Serialize;
use stream_throttle::{ThrottlePool, ThrottleRate};
use tokio_core::reactor::{Core, Timeout};

#[derive(Debug)]
struct Config {
    owner: String,
    repo: String,
    token: String,
    output_directory: PathBuf,
}
impl Config {
    fn from_args() -> Self {
        let matches = clap_app!((env!("CARGO_PKG_NAME")) =>
            (version: env!("CARGO_PKG_VERSION"))
            (author: env!("CARGO_PKG_AUTHORS"))
            (about: env!("CARGO_PKG_DESCRIPTION"))
            (@arg OWNER: -O --owner +required +takes_value "Repository owner to fetch data for")
            (@arg REPO: -r --repository +required +takes_value "Repository name to fetch data for")
            (@arg TOKEN: -t --token +required +takes_value "Github API token to use")
            (@arg OUTPUT_DIR: -o --("output-directory") +required +takes_value "Directory to output the data to")
        ).get_matches();
        Config {
            owner: matches.value_of("OWNER").unwrap().to_string(),
            repo: matches.value_of("REPO").unwrap().to_string(),
            output_directory: PathBuf::from(matches.value_of("OUTPUT_DIR").unwrap()),
            token: matches.value_of("TOKEN").unwrap().to_string(),
        }
    }
}

fn handle_issues<C>(
    cfg: &Config,
    core: &mut Core,
    github: &Github<C>,
) -> Result<(Vec<Issue>, Vec<u64>), Box<error::Error>>
  where C: Clone + Connect
{
    let repo = github.repo(cfg.owner.clone(), cfg.repo.clone());
    let issues_stream = repo.issues()
        .iter(&IssueListOptions::builder()
            .state(State::All)
            .asc()
            .per_page(100)
            .build());
    let (issues, pr_nums): (Vec<_>, Vec<_>) = core.run(issues_stream.collect())?
        .into_iter()
        .partition(|i| !i.pull_request.is_some());

    println!("Issues: {:?}", issues.len());
    Ok((issues, pr_nums.into_iter().map(|i| i.number).collect()))
}

fn get_pull<C>(repo: Repository<C>, n: u64) -> Box<Future<Item=Pull, Error=std::io::Error>>
  where C: Clone + Connect
{
    Box::new(repo.pulls().get(n).get().or_else(move |e| match e.kind() {
        &ErrorKind::RateLimit { reset: dt } => {
            let mut core = Core::new().expect("reactor fail");
            future::result(Timeout::new(dt, &core.handle()))
                .and_then(move |_| get_pull(repo, n))
        }
        _ => panic!("{}", e),
    }))
}

fn handle_pulls<C>(
    pull_nums: Vec<u64>,
    cfg: &Config,
    github: &Github<C>,
) -> Result<Vec<Pull>, Box<error::Error>>
  where C: Clone + Connect
{
    let pool = ThrottlePool::new(ThrottleRate::new(20, Duration::from_secs(1)));
    let pull_futs = pull_nums.into_iter().map(|n| {
        println!("Pull: {}", n);
        let repo = github.repo(cfg.owner.clone(), cfg.repo.clone());
        get_pull(repo, n)
    });
    let mut core = Core::new().expect("reactor fail");
    Ok(core.run(future::join_all(pull_futs))?)
}

fn serialize_to_file<D>(data: D, filename: &Path) -> Result<(), Box<error::Error>>
where
    D: Serialize,
{
    let mut file = fs::File::create(filename)?;
    data.serialize(&mut Serializer::new(&mut file))?;
    Ok(())
}

fn run() -> Result<(), Box<error::Error>> {
    let cfg = Config::from_args();

    let mut core = Core::new().expect("reactor fail");
    let github = Github::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        Credentials::Token(cfg.token.clone()),
    );

    let out_dir = cfg.output_directory.join(
        format!("{}/{}", cfg.owner, cfg.repo),
    );
    fs::create_dir_all(&out_dir)?;

    let (issues, pr_nums) = handle_issues(&cfg, &mut core, &github)?;
    serialize_to_file(&issues, &out_dir.join("issues.msgpack"))?;

    println!("Pulls: {}", pr_nums.len());
    let pulls = handle_pulls(pr_nums, &cfg, &github)?;
    serialize_to_file(&pulls, &out_dir.join("pulls.msgpack"))?;

    Ok(())
}

fn main() {
    run().unwrap();

    // let mut buf = Vec::new();
    // issues.serialize(&mut Serializer::new(&mut buf)).unwrap();
    // println!("{:?}", buf);
}
