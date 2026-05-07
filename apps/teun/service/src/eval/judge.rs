use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeVerdict {
    pub score: u8,
    pub verdict: String,
    pub reasoning: String,
}

pub async fn judge_answer(
    client: &reqwest::Client,
    anthropic_api_key: &str,
    judge_model: &str,
    judge_prompt_template: &str,
    question: &str,
    expected_key_points: &[String],
    expected_category: Option<&str>,
    actual_answer: &str,
    actual_rationale: &str,
    actual_sources: &str,
    actual_category: &str,
) -> Result<JudgeVerdict> {
    let key_points = expected_key_points.join("\n  - ");
    let expected_cat = expected_category.unwrap_or("niet gespecificeerd");

    let prompt = judge_prompt_template
        .replace("{question}", question)
        .replace("{expected_category}", expected_cat)
        .replace("{expected_key_points}", &key_points)
        .replace("{actual_answer}", actual_answer)
        .replace("{actual_rationale}", actual_rationale)
        .replace("{actual_sources}", actual_sources)
        .replace("{actual_category}", actual_category);

    let body = serde_json::json!({
        "model": judge_model,
        "max_tokens": 1024,
        "messages": [{ "role": "user", "content": prompt }]
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", anthropic_api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to call Anthropic API")?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API returned {status}: {text}");
    }

    let resp: serde_json::Value = response.json().await.context("Failed to parse Anthropic response")?;

    let text = resp["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No text in Anthropic response"))?;

    let json_str = extract_json(text)?;
    let verdict: JudgeVerdict = serde_json::from_str(&json_str)
        .with_context(|| format!("Failed to parse judge verdict JSON. Extracted: {json_str}"))?;

    if verdict.score < 1 || verdict.score > 5 {
        anyhow::bail!("Judge score {} out of range 1-5", verdict.score);
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
    if let Some(start) = text.find("```") {
        let json_start = start + 3;
        let json_start = text[json_start..]
            .find('\n')
            .map(|i| json_start + i + 1)
            .unwrap_or(json_start);
        if let Some(end) = text[json_start..].find("```") {
            return Ok(text[json_start..json_start + end].trim().to_string());
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].to_string());
        }
    }
    anyhow::bail!("No JSON found in judge response: {text}")
}

pub const DEFAULT_JUDGE_TEMPLATE: &str = r#"Je bent een kwaliteitsbeoordelaar voor een hypotheek acceptatie chatbot van DMFCO.

Beoordeel het volgende antwoord op correctheid, volledigheid en bruikbaarheid.

## Vraag
{question}

## Verwacht
- Categorie: {expected_category}
- Kernpunten die aan bod moeten komen:
  - {expected_key_points}

## Antwoord van de chatbot
- Categorie: {actual_category}
- Antwoord: {actual_answer}
- Onderbouwing: {actual_rationale}
- Bronnen: {actual_sources}

## Beoordelingscriteria
Beoordeel op:
1. Correctheid: Klopt het antwoord met het hypotheekbeleid?
2. Volledigheid: Komen alle verwachte kernpunten aan bod?
3. Bronverwijzing: Zijn de bronnen relevant en correct?
4. Categorie: Klopt de categorie (standard / mandaat_uitzondering / doorverwijzen_speciale_afhandeling)?

Geef een score van 1-5:
- 5: Perfect antwoord, correct en volledig
- 4: Goed antwoord, kleine onvolkomenheden
- 3: Acceptabel, maar mist belangrijke details
- 2: Onvolledig of deels onjuist
- 1: Onjuist of misleidend

Geef een verdict: "pass" (score 4-5), "partial" (score 3), of "fail" (score 1-2).

Antwoord ALLEEN met JSON, geen andere tekst:
{"score": <1-5>, "verdict": "<pass|partial|fail>", "reasoning": "<korte uitleg in het Nederlands>"}"#;
