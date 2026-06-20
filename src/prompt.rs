pub struct CoderInput<'a> {
    pub task: &'a str,
    pub context: Option<&'a str>,
    pub language: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
}

pub fn coder_prompt(input: CoderInput<'_>) -> String {
    let system = input.system_prompt.unwrap_or(
        "You are an expert coding assistant working in Codex-style mode. \
         Produce concise, production-ready code and short explanations. \
         Prefer complete files and runnable examples."
    );

    let mut prompt = String::new();
    prompt.push_str(system);
    prompt.push('\n');

    if let Some(language) = input.language {
        prompt.push_str("Language: ");
        prompt.push_str(language);
        prompt.push('\n');
    }

    prompt.push_str("Task:\n");
    prompt.push_str(input.task);
    prompt.push('\n');

    if let Some(context) = input.context {
        prompt.push_str("\nContext:\n");
        prompt.push_str(context);
        prompt.push('\n');
    }

    prompt.push_str(
        "\nReturn the final answer with code blocks if relevant. \
         If you need to edit an existing file, show the full updated file or the diff."
    );

    prompt
}

/// Build a single prompt string from an OpenAI-style messages array.
/// System messages are used as a prefix; user/assistant turns are interleaved.
pub fn conversation_prompt(messages: &[(String, String)]) -> String {
    let mut prompt = String::new();

    let system_parts: Vec<&str> = messages
        .iter()
        .filter(|(role, _)| role == "system")
        .map(|(_, content)| content.as_str())
        .collect();

    if !system_parts.is_empty() {
        prompt.push_str(&system_parts.join("\n"));
        prompt.push('\n');
    }

    for (role, content) in messages.iter().filter(|(r, _)| r != "system") {
        match role.as_str() {
            "user" => {
                prompt.push_str("User: ");
            }
            "assistant" => {
                prompt.push_str("Assistant: ");
            }
            _ => {
                prompt.push_str(role);
                prompt.push_str(": ");
            }
        }
        prompt.push_str(content);
        prompt.push('\n');
    }

    prompt
}
