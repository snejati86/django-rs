//! The `dumpdata` management command.
//!
//! Serializes model data to JSON for backup or fixture creation.
//! This mirrors Django's `dumpdata` command.

use async_trait::async_trait;
use django_rs_core::{DjangoError, Settings};

use crate::command::ManagementCommand;
use crate::serialization::{PrettyJsonSerializer, Serializer};

/// Outputs serialized model data to stdout or a file.
///
/// Takes an optional `app_label.ModelName` argument to restrict output
/// to a specific model. Supports `--indent` for pretty-printed output
/// and `--output` to write to a file instead of stdout.
pub struct DumpdataCommand;

/// Serializes the given objects and writes them to the specified output.
///
/// If `output_path` is `None`, writes to stdout. If `indent` is true,
/// uses pretty-printed JSON formatting.
pub async fn dump_data(
    objects: &[serde_json::Value],
    output_path: Option<&str>,
    indent: bool,
) -> Result<String, DjangoError> {
    let serializer: Box<dyn Serializer> = if indent {
        Box::new(PrettyJsonSerializer)
    } else {
        Box::new(crate::serialization::JsonSerializer)
    };

    let result = serializer.serialize(objects)?;

    if let Some(path) = output_path {
        tokio::fs::write(path, &result).await.map_err(|e| {
            DjangoError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to write to {path}: {e}"),
            ))
        })?;
        tracing::info!("Data written to {path}");
    }

    Ok(result)
}

/// Parses a model specifier string into (app_label, model_name).
///
/// The specifier can be either `app_label` (all models in the app) or
/// `app_label.ModelName` (a specific model).
pub fn parse_model_specifier(spec: &str) -> (String, Option<String>) {
    if let Some((app, model)) = spec.split_once('.') {
        (app.to_string(), Some(model.to_string()))
    } else {
        (spec.to_string(), None)
    }
}

#[async_trait]
impl ManagementCommand for DumpdataCommand {
    fn name(&self) -> &'static str {
        "dumpdata"
    }

    fn help(&self) -> &'static str {
        "Serialize model data to JSON"
    }

    fn add_arguments(&self, cmd: clap::Command) -> clap::Command {
        cmd.arg(
            clap::Arg::new("app_label")
                .help("App label or app_label.ModelName to dump")
                .num_args(0..),
        )
        .arg(
            clap::Arg::new("indent")
                .long("indent")
                .action(clap::ArgAction::SetTrue)
                .help("Use pretty-printed JSON output"),
        )
        .arg(
            clap::Arg::new("output")
                .long("output")
                .short('o')
                .help("Output file path (default: stdout)"),
        )
        .arg(
            clap::Arg::new("database")
                .long("database")
                .default_value("default")
                .help("Database alias to dump from"),
        )
    }

    async fn handle(
        &self,
        matches: &clap::ArgMatches,
        _settings: &Settings,
    ) -> Result<(), DjangoError> {
        let indent = matches.get_flag("indent");
        let output = matches.get_one::<String>("output").map(String::as_str);
        let database = matches
            .get_one::<String>("database")
            .map_or("default", String::as_str);
        let app_labels: Vec<&String> = matches
            .get_many::<String>("app_label")
            .map_or_else(Vec::new, Iterator::collect);

        tracing::info!("Dumping data from database '{database}'");

        if app_labels.is_empty() {
            tracing::info!("Dumping all models");
        } else {
            for spec in &app_labels {
                let (app, model) = parse_model_specifier(spec);
                if let Some(model_name) = &model {
                    tracing::info!("Dumping {app}.{model_name}");
                } else {
                    tracing::info!("Dumping all models in app '{app}'");
                }
            }
        }

        // In a full implementation, this would query the database and serialize
        // the results. For now, we produce an empty array placeholder.
        let objects: Vec<serde_json::Value> = Vec::new();
        let serialized = dump_data(&objects, output, indent).await?;

        if output.is_none() {
            // Write to stdout via spawn_blocking to avoid blocking the runtime
            let output_clone = serialized;
            tokio::task::spawn_blocking(move || {
                println!("{output_clone}");
            })
            .await
            .map_err(|e| DjangoError::InternalServerError(e.to_string()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_model_specifier_with_model() {
        let (app, model) = parse_model_specifier("auth.User");
        assert_eq!(app, "auth");
        assert_eq!(model, Some("User".to_string()));
    }

    #[test]
    fn test_parse_model_specifier_app_only() {
        let (app, model) = parse_model_specifier("auth");
        assert_eq!(app, "auth");
        assert!(model.is_none());
    }

    #[tokio::test]
    async fn test_dump_data_compact() {
        let objects = vec![json!({"pk": 1, "name": "test"})];
        let result = dump_data(&objects, None, false).await.unwrap();
        assert!(!result.contains('\n'));
        assert!(result.contains("\"pk\":1"));
    }

    #[tokio::test]
    async fn test_dump_data_pretty() {
        let objects = vec![json!({"pk": 1, "name": "test"})];
        let result = dump_data(&objects, None, true).await.unwrap();
        assert!(result.contains('\n'));
        assert!(result.contains("\"pk\": 1"));
    }

    #[tokio::test]
    async fn test_dump_data_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.json");
        let path_str = path.to_str().unwrap();

        let objects = vec![json!({"pk": 1})];
        dump_data(&objects, Some(path_str), false).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("\"pk\":1"));
    }

    #[tokio::test]
    async fn test_dump_data_empty() {
        let result = dump_data(&[], None, false).await.unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_command_metadata() {
        let cmd = DumpdataCommand;
        assert_eq!(cmd.name(), "dumpdata");
        assert_eq!(cmd.help(), "Serialize model data to JSON");
    }
}
