use anyhow::{Context, Result};
use serde::Deserialize;

use super::types::SourceVerdict;
use crate::agent::types::MortgageAnswer;

#[derive(Debug, Deserialize)]
pub struct LlmVerdict {
    pub score: u8,
    pub reasoning: String,
}

#[tracing::instrument(
    name = "ai.faithfulness_judge",
    skip(client, api_key, answer, source_verdicts),
    fields(
        gen_ai.system = "anthropic",
        gen_ai.operation.name = "faithfulness_judge",
        gen_ai.request.model = %model,
    )
)]
pub async fn call_faithfulness_judge(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    question: &str,
    answer: &MortgageAnswer,
    source_verdicts: &[SourceVerdict],
) -> Result<LlmVerdict> {
    let sources_text = answer
        .sources
        .iter()
        .zip(source_verdicts.iter())
        .map(|(src, verdict)| {
            let status = serde_json::to_string(&verdict.status).unwrap_or_default();
            format!(
                "- {}/{}: {} [verificatie: {}]",
                src.document,
                src.section,
                src.quote.as_deref().unwrap_or("(geen citaat)"),
                status.trim_matches('"'),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let category = serde_json::to_string(&answer.category).unwrap_or_default();

    let prompt = JUDGE_PROMPT
        .replace("{question}", question)
        .replace("{answer}", &answer.answer)
        .replace("{rationale}", &answer.rationale)
        .replace("{sources}", &sources_text)
        .replace("{category}", category.trim_matches('"'));

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 512,
        "messages": [{ "role": "user", "content": prompt }]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Anthropic API request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API error {status}: {text}");
    }

    let resp_json: serde_json::Value = resp.json().await.context("Failed to parse response")?;

    let text = resp_json["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No text in Anthropic response"))?;

    parse_verdict(text)
}

fn parse_verdict(text: &str) -> Result<LlmVerdict> {
    let json_str = extract_json(text)?;
    let verdict: LlmVerdict =
        serde_json::from_str(&json_str).context("Failed to parse judge verdict JSON")?;
    if verdict.score < 1 || verdict.score > 100 {
        anyhow::bail!("Judge score {} out of range 1-100", verdict.score);
    }
    Ok(verdict)
}

fn extract_json(text: &str) -> Result<String> {
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            return Ok(text[json_start..json_start + end].trim().to_string());
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("No JSON found in judge response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_markdown_block() {
        let text = r#"Here is the result:
```json
{"score": 85, "reasoning": "Goed antwoord"}
```"#;
        let result = extract_json(text).unwrap();
        assert_eq!(result, r#"{"score": 85, "reasoning": "Goed antwoord"}"#);
    }

    #[test]
    fn extract_json_raw_object() {
        let text = r#"Some preamble {"score": 50, "reasoning": "Matig"} trailing"#;
        let result = extract_json(text).unwrap();
        assert_eq!(result, r#"{"score": 50, "reasoning": "Matig"}"#);
    }

    #[test]
    fn extract_json_no_json() {
        let text = "No JSON here at all";
        assert!(extract_json(text).is_err());
    }

    #[test]
    fn extract_json_only_braces() {
        let text = r#"{"score": 92, "reasoning": "Uitstekend"}"#;
        let result = extract_json(text).unwrap();
        assert_eq!(result, text);
    }

    #[test]
    fn parse_verdict_valid() {
        let text = r#"{"score": 85, "reasoning": "Goed onderbouwd antwoord"}"#;
        let verdict = parse_verdict(text).unwrap();
        assert_eq!(verdict.score, 85);
        assert_eq!(verdict.reasoning, "Goed onderbouwd antwoord");
    }

    #[test]
    fn parse_verdict_in_markdown() {
        let text = r#"```json
{"score": 42, "reasoning": "Onvoldoende"}
```"#;
        let verdict = parse_verdict(text).unwrap();
        assert_eq!(verdict.score, 42);
    }

    #[test]
    fn parse_verdict_score_out_of_range_zero() {
        let text = r#"{"score": 0, "reasoning": "Invalid"}"#;
        assert!(parse_verdict(text).is_err());
    }

    #[test]
    fn parse_verdict_score_out_of_range_high() {
        let text = r#"{"score": 101, "reasoning": "Too high"}"#;
        assert!(parse_verdict(text).is_err());
    }

    #[test]
    fn parse_verdict_invalid_json() {
        let text = "not json at all";
        assert!(parse_verdict(text).is_err());
    }
}

const JUDGE_PROMPT: &str = r#"Je bent een kwaliteitsbeoordelaar voor een hypotheek acceptatie chatbot van DMFCO.
Beoordeel of het antwoord trouw is aan de geciteerde bronnen en binnen het acceptatiebeleid blijft.

## Vraag van de adviseur
{question}

## Antwoord van de chatbot
Categorie: {category}
Antwoord: {answer}
Redenering: {rationale}

## Geciteerde bronnen (met verificatiestatus)
{sources}

## Verificatiestatus uitleg
Elke bron heeft een verificatiestatus:
- **ok**: document gevonden, regels bestaan, citaat komt overeen met brontekst. Dit is een BETROUWBARE bron.
- **document_not_found**: document bestaat niet. Dit is een ONBETROUWBARE bron.
- **line_range_out_of_bounds**: de opgegeven regels bestaan niet. Dit is een ONBETROUWBARE bron.
- **quote_mismatch**: citaat komt niet overeen met de tekst op de opgegeven regels. Dit is een ONBETROUWBARE bron.

BELANGRIJK: bronnen met een andere status dan "ok" zijn NIET geverifieerd. Beschouw deze als onbetrouwbaar. Beweringen die alleen steunen op niet-geverifieerde bronnen tellen als ononderbouwd.

## Beoordelingscriteria
1. **Trouwheid aan geverifieerde bronnen**: Volgt het antwoord logisch uit bronnen met status "ok"? Beweringen gebaseerd op niet-geverifieerde bronnen zijn ononderbouwd.
2. **Beleidscompliance**: Blijft het antwoord binnen de scope van het acceptatiebeleid?
3. **Consistentie**: Klopt de redenering met het antwoord en de geverifieerde bronnen?

Geef een score van 1-100:
- 90-100: Uitstekend — volledig trouw aan geverifieerde bronnen, geen onondersteunde beweringen
- 70-89: Goed — lichte afwijkingen maar kern correct, bronnen overwegend geverifieerd
- 50-69: Matig — enkele onondersteunde beweringen of niet-geverifieerde bronnen
- 30-49: Onvoldoende — meerdere niet-geverifieerde bronnen of onjuiste beweringen
- 1-29: Slecht — antwoord grotendeels niet te herleiden naar geverifieerde bronnen

Antwoord ALLEEN met JSON, geen andere tekst:
{"score": <1-100>, "reasoning": "<korte uitleg in het Nederlands, max 200 woorden>"}"#;
