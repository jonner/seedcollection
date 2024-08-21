use crate::output::OutputFormat;
use clap::{Parser, Subcommand, ValueEnum};
use libseed::taxonomy;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Login to the database")]
    Login {
        #[arg(short, long)]
        username: Option<String>,
        #[arg(short, long)]
        database: Option<PathBuf>,
    },
    #[command(about = "Log out of the database")]
    Logout,
    #[command(
        about = "Show current config status",
        after_help = "Shows the current configuration, including the path to the database and the logged in user"
    )]
    Status,
    #[command(
        about = "Manage seed sources",
        after_help = "A seed source is a way to track the origin of a particular seed sample. It could be a geographical location where it was collected, or a commercial supplier, or anything else you want to track."
    )]
    #[clap(alias = "source")]
    Sources {
        #[command(subcommand)]
        command: SourceCommands,
    },
    #[command(
        about = "Manage seed samples",
        after_help = "A seed sample is a specific acquisition of a single type of seeds. It could be a purchased pack of seeds, or a single collection event from a specific location."
    )]
    #[clap(alias = "sample")]
    Samples {
        #[command(subcommand)]
        command: SampleCommands,
    },
    #[command(
        about = "Manage seed projects",
        after_help = "A project is simply a way to keep track of groups of seed samples. For example, if you want to use certain seed samples for a particular planting event, you could create a project for that event and add all of those samples to the project."
    )]
    #[clap(alias = "project")]
    Projects {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    #[command(about = "Query taxonomy")]
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    #[command(about = "Administrative commands")]
    Admin {
        #[command(subcommand)]
        command: AdminCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProjectCommands {
    #[command(about = "List all projects")]
    List {},
    #[command(about = "Add a new project to the database")]
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long)]
        userid: Option<i64>,
    },
    #[command(
        about="Modify properties of a project",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description"]),
        ))]
    #[clap(alias = "edit")]
    Modify {
        id: i64,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
    },
    #[command(about = "Remove a project from the database")]
    Remove { id: i64 },
    #[command(about = "Add a new sample to the project")]
    AddSample {
        #[arg(short, long)]
        project: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Remove an existing sample from the project")]
    RemoveSample {
        #[arg(short, long)]
        project: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Show all details about a project")]
    Show {
        id: i64,
        #[arg(short, long)]
        full: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SourceCommands {
    #[command(about = "List all sources")]
    List {
        #[arg(short, long)]
        full: bool,
        #[arg(long)]
        filter: Option<String>,
    },
    #[command(about = "Show details about a single source")]
    Show { id: i64 },
    #[command(about = "Add a new source to the database")]
    Add {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
        #[arg(long)]
        userid: Option<i64>,
    },
    #[command(about = "Remove an existing source from the database")]
    Remove { id: i64 },
    #[command(
        about="Modify properties of a source",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description", "latitude", "longitude"]),
        ))]
    #[clap(alias = "edit")]
    Modify {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SampleSortField {
    Id,
    Taxon,
    Name,
    Source,
    Date,
}

#[derive(Subcommand, Debug)]
pub enum SampleCommands {
    #[command(about = "List all samples")]
    List {
        #[arg(short, long)]
        full: bool,
        #[arg(short, long)]
        user: bool,
        #[arg(short, long)]
        limit: Option<String>,
        #[arg(short, long, value_delimiter = ',')]
        sort: Option<Vec<SampleSortField>>,
        #[arg(short, long, help = "Reverse sort order")]
        reverse: bool,
        #[arg(value_enum, short, long, default_value_t = OutputFormat::Table)]
        output: OutputFormat,
    },
    #[command(about = "Show details for a single sample")]
    Show {
        id: i64,
    },
    #[command(about = "Add a new sample to the database")]
    Add {
        #[arg(short, long)]
        taxon: Option<i64>,
        #[arg(short, long)]
        source: Option<i64>,
        #[arg(short, long)]
        month: Option<u32>,
        #[arg(short, long)]
        year: Option<u32>,
        #[arg(short, long)]
        quantity: Option<i64>,
        #[arg(short, long)]
        notes: Option<String>,
        #[arg(short = '?', long)]
        uncertain: bool,
        #[arg(short, long)]
        userid: Option<i64>,
    },
    #[command(about = "Remove an existing sample from the database")]
    Remove {
        id: i64,
    },
    #[command(about = "Modify properties of a sample")]
    #[clap(alias = "edit")]
    Modify {
        id: i64,
        #[arg(long)]
        taxon: Option<i64>,
        #[arg(long)]
        source: Option<i64>,
        #[arg(short, long)]
        month: Option<u16>,
        #[arg(short, long)]
        year: Option<u16>,
        #[arg(short, long)]
        quantity: Option<u32>,
        #[arg(short, long)]
        notes: Option<String>,
        #[arg(long)]
        certain: bool,
        #[arg(long, conflicts_with("certain"))]
        uncertain: bool,
    },
    Stats,
}

#[derive(Subcommand, Debug)]
pub enum TaxonomyCommands {
    #[command(about = "Find a taxon in the database")]
    Find {
        #[arg(long, help = "Only show taxa with the given rank (e.g. 'family')")]
        rank: Option<taxonomy::Rank>,
        #[arg(long, help = "Only show taxa in the given genus")]
        genus: Option<String>,
        #[arg(long, help = "Only show taxa in the given species")]
        species: Option<String>,
        #[arg(long, help = "Show taxa with the given string in any field")]
        any: Option<String>,
        #[arg(long, help = "Show only taxa found in Minnesota")]
        minnesota: bool,
    },
    #[command(about = "Show information about a taxon")]
    Show { id: i64 },
}

#[derive(Subcommand, Debug)]
pub enum AdminCommands {
    #[command(
        about = "Manage users",
        after_help = "The database can track the collections of multiple users. This command can be used to manage the users defined for this database."
    )]
    #[clap(alias = "user")]
    Users {
        #[command(subcommand)]
        command: UserCommands,
    },
    #[command(
        about = "Manage germination codes",
        after_help = "Manage information about germination instructions for different seeds."
    )]
    Germination {
        #[command(subcommand)]
        command: GerminationCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum UserCommands {
    #[command(about = "List all users")]
    List {},
    #[command(about = "Add a new user to the database")]
    Add {
        #[arg(long, help = "A unique username for the user")]
        username: String,
        #[arg(long, help = "Email address for the user")]
        email: String,
        #[arg(
            long,
            help = "Optional path to a file containing the user's password. If not given, you will be prompted for your password"
        )]
        passwordfile: Option<PathBuf>,
    },
    #[command(about = "Remove an existing user from the database")]
    Remove {
        #[arg(help = "The user ID of the user to remove from the database")]
        id: i64,
    },
    #[command(
        about="Modify properties about a user",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["username", "change_password"]),
        ))]
    #[clap(alias = "edit")]
    Modify {
        #[arg(help = "The user id of the user to modify")]
        id: i64,
        #[arg(long, help = "A new username for the user")]
        username: Option<String>,
        #[arg(long, short = 'p', help = "Change the user's password")]
        change_password: bool,
        #[arg(
            long,
            short = 'f',
            requires("change_password"),
            help = "Optional path to a file containing the new password. If not given, you will be prompted for your password"
        )]
        passwordfile: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GerminationCommands {
    #[command(about = "List all germination codes")]
    List {},
    #[command(about = "Modify properties of a germination code")]
    #[clap(alias = "edit")]
    Modify {
        id: i64,
        #[arg(
            long,
            short,
            help = "A short code representing the germination requirements"
        )]
        code: Option<String>,
        #[arg(long, short, help = "Summary of the germination requirements")]
        summary: Option<String>,
        #[arg(
            long,
            short,
            help = "Longer description of the germination requirements"
        )]
        description: Option<String>,
    },
}
