use chrono::Local;
use clap::{Parser, Subcommand};
use confy;
use reqwest::blocking::Client;
use reqwest::{Error, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    Configure {
        #[arg(short, long)]
        oura_token: String,
    },
    Show {},
    Latest {},
    Score {
        #[arg(short, long)]
        start_date: String,
        #[arg(short, long)]
        end_date: String,
        #[arg(short, long, default_value = "text")]
        output_format: String, // text or json
    },
}
#[derive(Default, Serialize, Deserialize, Clone)]
struct CliConfig {
    oura_token: String,
}

const CONFIG_APP_NAME: &'static str = "oura-cli";

fn main() {
    let args = Cli::parse();
    let mut config: CliConfig = confy::load(CONFIG_APP_NAME, None).unwrap();

    if let Some(command) = args.command {
        match command {
            Commands::Configure { oura_token } => {
                config.oura_token = oura_token;
                confy::store(CONFIG_APP_NAME, None, &config).unwrap();
                println!("Oura token has been configured.");
            }
            Commands::Show {} => {
                println!("Oura token: {}", config.oura_token);
            }
            Commands::Latest {} => {
                let today = Local::now().format("%Y-%m-%d").to_string();

                match get_sleep_score(&config, &today, &today) {
                    Ok(scores) => {
                        for score in scores {
                            println!("{}", print_sleep_score_as_csv(
                                score["date"].as_str().unwrap(),
                                score["score"].to_string().as_str(),
                            ));
                        }
                    }
                    Err(e) => eprintln!("Error fetching sleep score: {}", e),
                }
            }
            Commands::Score {
                start_date,
                end_date,
                output_format,
            } => match get_sleep_score(&config, &start_date, &end_date) {
                Ok(scores) => {
                    if output_format == "text" {
                        for score in scores {
                            println!("{}", print_sleep_score_as_csv(
                                score["date"].as_str().unwrap(),
                                score["score"].to_string().as_str(),
                            ));
                        }
                    } else {
                        println!("{}", print_sleep_score_as_json(&scores));
                    }
                }
                Err(e) => eprintln!("Error fetching sleep score: {}", e),
            },
        }
        return;
    }
}
#[derive(Deserialize)]
struct SleepData {
    data: Vec<SleepEntry>,
}

#[derive(Deserialize)]
struct SleepEntry {
    day: String,
    score: u32,
}

fn print_sleep_score_as_json(scores: &Vec<Value>) -> String {
    serde_json::to_string(&scores).expect("Failed to serialize scores to JSON")
}

fn print_sleep_score_as_csv(date: &str, score: &str) -> String {
    format!("\"{}\",{}\n", date, score)
}
fn get_sleep_score(
    cli_config: &CliConfig,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    let base_url = std::env::var("OURA_API_URL").unwrap_or_else(|_| "https://api.ouraring.com".to_string());
    let url = format!(
        "{}/v2/usercollection/daily_sleep?start_date={}&end_date={}",
        base_url, start_date, end_date
    );

    let token = cli_config.oura_token.as_str();

    if token.is_empty() {
        let client = Client::new();
        let err = client.get("http://localhost:1")  // Using port 1 to guarantee connection refused
            .send()
            .unwrap_err();
        return Err(err);
    }

    let client = Client::new();
    let response = client.get(&url).bearer_auth(token).send()?;
    
    // Handle HTTP status errors
    if !response.status().is_success() {
        let err = response.error_for_status().unwrap_err();
        return Err(err);
    }
    
    let response_text = response.text()?;
    let mut sleep_scores = Vec::new();
    if let Ok(sleep_data) = serde_json::from_str::<SleepData>(&response_text) {
        sleep_scores = sleep_data
            .data
            .into_iter()
            .map(|entry| json!({ "date": entry.day, "score": entry.score }))
            .collect();

        sleep_scores.sort_by(|a, b| a["date"].as_str().cmp(&b["date"].as_str()));
    }

    Ok(sleep_scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;
    use wiremock::{Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header, query_param};

    #[test]
    fn test_cli_configure_command() {
        let args = Cli::parse_from(["oura-cli", "configure", "--oura-token", "test_token"]);
        match args.command.unwrap() {
            Commands::Configure { oura_token } => {
                assert_eq!(oura_token, "test_token");
            }
            _ => panic!("Expected Configure command"),
        }
    }

    #[test]
    fn test_cli_show_command() {
        let args = Cli::parse_from(["oura-cli", "show"]);
        match args.command.unwrap() {
            Commands::Show {} => {}
            _ => panic!("Expected Show command"),
        }
    }

    #[test]
    fn test_cli_latest_command() {
        let args = Cli::parse_from(["oura-cli", "latest"]);
        match args.command.unwrap() {
            Commands::Latest {} => {}
            _ => panic!("Expected Latest command"),
        }
    }

    #[test]
    fn test_cli_score_command() {
        let args = Cli::parse_from([
            "oura-cli", "score",
            "--start-date", "2024-01-01",
            "--end-date", "2024-01-02",
            "--output-format", "json"
        ]);
        match args.command.unwrap() {
            Commands::Score { start_date, end_date, output_format } => {
                assert_eq!(start_date, "2024-01-01");
                assert_eq!(end_date, "2024-01-02");
                assert_eq!(output_format, "json");
            }
            _ => panic!("Expected Score command"),
        }
    }

    #[test]
    fn test_cli_score_command_default_output_format() {
        let args = Cli::parse_from([
            "oura-cli", "score",
            "--start-date", "2024-01-01",
            "--end-date", "2024-01-02"
        ]);
        match args.command.unwrap() {
            Commands::Score { start_date, end_date, output_format } => {
                assert_eq!(start_date, "2024-01-01");
                assert_eq!(end_date, "2024-01-02");
                assert_eq!(output_format, "text");
            }
            _ => panic!("Expected Score command"),
        }
    }

    #[test]
    fn test_config_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config = CliConfig {
            oura_token: "test_token".to_string(),
        };
        
        let config_path = temp_dir.path().join("config.toml");
        confy::store_path(&config_path, config.clone()).unwrap();
        
        let loaded: CliConfig = confy::load_path(&config_path).unwrap();
        assert_eq!(loaded.oura_token, "test_token");
    }

    #[test]
    fn test_config_default_values() {
        let config = CliConfig::default();
        assert_eq!(config.oura_token, "");
    }

    #[test]
    fn test_config_invalid_path() {
        let invalid_path = PathBuf::from("/nonexistent/path/config.toml");
        let result: Result<CliConfig, confy::ConfyError> = confy::load_path(&invalid_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_sleep_score_success() {
        let mock_response_text = r#"{
            "data": [
                {
                    "day": "2024-01-01",
                    "score": 85
                }
            ]
        }"#;

        // Start a mock server
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mock_server = rt.block_on(async {
            let server = wiremock::MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v2/usercollection/daily_sleep"))
                .and(query_param("start_date", "2024-01-01"))
                .and(query_param("end_date", "2024-01-01"))
                .and(header("Authorization", "Bearer test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_string(mock_response_text))
                .expect(1)
                .mount(&server)
                .await;
            server
        });
        let mock_server_url = mock_server.uri();

        // Mock is already set up in the block_on above

        let config = CliConfig {
            oura_token: "test_token".to_string(),
        };

        // Set the mock server URL as the API base URL for testing
        std::env::set_var("OURA_API_URL", &mock_server_url);

        let result = get_sleep_score(&config, "2024-01-01", "2024-01-01").unwrap();

        // Verify the response
        assert_eq!(result[0]["date"], "2024-01-01");
        assert_eq!(result[0]["score"], 85);

        // Verify that the mock was called exactly once
        // Request verification is handled by wiremock's expect(1)
    }

    #[test]
    fn test_print_sleep_score_as_json() {
        let scores = vec![
            json!({
                "date": "2024-01-01",
                "score": 85
            }),
            json!({
                "date": "2024-01-02",
                "score": 90
            })
        ];
        
        let output = print_sleep_score_as_json(&scores);
        assert_eq!(output, r#"[{"date":"2024-01-01","score":85},{"date":"2024-01-02","score":90}]"#);
    }

    #[test]
    fn test_print_sleep_score_as_json_empty() {
        let scores = vec![];
        let output = print_sleep_score_as_json(&scores);
        assert_eq!(output, "[]");
    }

    #[test]
    fn test_print_sleep_score_as_csv() {
        let output = print_sleep_score_as_csv("2024-01-01", "85");
        assert_eq!(output, "\"2024-01-01\",85\n");
    }

    #[test]
    fn test_print_sleep_score_as_csv_with_comma() {
        let output = print_sleep_score_as_csv("2024-01-01", "85,90");
        assert_eq!(output, "\"2024-01-01\",85,90\n");
    }

    #[test]
    fn test_get_sleep_score_invalid_response() {
        let mock_response_text = r#"{
            "invalid": "response"
        }"#;

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mock_server = rt.block_on(async {
            let server = wiremock::MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v2/usercollection/daily_sleep"))
                .and(query_param("start_date", "2024-01-01"))
                .and(query_param("end_date", "2024-01-01"))
                .and(header("Authorization", "Bearer test_token"))
                .respond_with(ResponseTemplate::new(200).set_body_string(mock_response_text))
                .mount(&server)
                .await;
            server
        });

        let config = CliConfig {
            oura_token: "test_token".to_string(),
        };

        std::env::set_var("OURA_API_URL", &mock_server.uri());
        let result = get_sleep_score(&config, "2024-01-01", "2024-01-01");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_get_sleep_score_api_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mock_server = rt.block_on(async {
            let server = wiremock::MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v2/usercollection/daily_sleep"))
                .and(query_param("start_date", "2024-01-01"))
                .and(query_param("end_date", "2024-01-01"))
                .and(header("Authorization", "Bearer test_token"))
                .respond_with(ResponseTemplate::new(401)
                    .set_body_string("Unauthorized")
                    .insert_header("content-type", "text/plain"))
                .expect(1)
                .mount(&server)
                .await;
            server
        });

        let config = CliConfig {
            oura_token: "test_token".to_string(),
        };

        std::env::set_var("OURA_API_URL", &mock_server.uri());
        let result = get_sleep_score(&config, "2024-01-01", "2024-01-01");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_sleep_score_empty_token() {
        let config = CliConfig {
            oura_token: "".to_string(),
        };
        
        let result = get_sleep_score(&config, "2024-01-01", "2024-01-01");
        assert!(result.is_err());
        assert!(result.unwrap_err().is_connect());
    }
}
