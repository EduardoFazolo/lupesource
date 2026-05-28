#!/usr/bin/env python3
"""
Lupe stop hook — auto-captures every user message and agent response.
Fires after each agent turn. Creates a checkpoint for the turn and saves the response.
Works with Claude Code, Codex CLI, and Cursor.
"""
import json
import os
import subprocess
import sys

LUPE = os.environ.get("LUPE_BIN", "/Users/eduardoverona/.cargo/bin/lupe")


def run_lupe(*args: str) -> bool:
    try:
        result = subprocess.run(
            [LUPE, *args],
            check=False,
            timeout=15,
            capture_output=True,
        )
        return result.returncode == 0
    except Exception:
        return False


def extract_messages(path: str) -> tuple[str | None, str | None, str | None]:
    """Return (last_user_text, last_assistant_text, model) from a transcript JSONL."""
    if not path or not os.path.exists(path):
        return None, None, None

    last_user = None
    last_assistant = None
    model = None

    try:
        with open(path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    entry = json.loads(line)
                    msg = entry.get("message", {}) or {}
                    role = msg.get("role") or entry.get("role")
                    content = msg.get("content") or entry.get("content", [])

                    if model is None:
                        model = msg.get("model") or entry.get("model")

                    if isinstance(content, list):
                        texts = [
                            c["text"]
                            for c in content
                            if isinstance(c, dict)
                            and c.get("type") == "text"
                            and c.get("text", "").strip()
                        ]
                        text = "\n".join(texts) if texts else None
                    elif isinstance(content, str) and content.strip():
                        text = content.strip()
                    else:
                        text = None

                    if not text:
                        continue

                    if role == "assistant":
                        last_assistant = text
                    elif role == "user":
                        last_user = text
                except json.JSONDecodeError:
                    continue
    except Exception:
        pass

    return last_user, last_assistant, model


def detect_agent(payload: dict, model: str | None) -> str:
    agent_name = os.environ.get("LUPE_AGENT_NAME")
    if not agent_name:
        if "transcript_path" in payload:
            agent_name = "claude-code"
        elif payload.get("last_assistant_message"):
            agent_name = "codex"
        else:
            agent_name = "unknown"
    agent_model = os.environ.get("LUPE_AGENT_MODEL") or model or "unknown"
    return f"{agent_name}/{agent_model}"


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        sys.exit(0)

    transcript_path = payload.get("transcript_path") or ""

    # Codex provides assistant message directly
    assistant_text = payload.get("last_assistant_message")

    # Parse transcript for user message (and assistant if not provided)
    user_text, transcript_assistant, model = extract_messages(transcript_path)
    if not assistant_text:
        assistant_text = transcript_assistant

    if not assistant_text or not assistant_text.strip():
        sys.exit(0)

    agent = detect_agent(payload, model)

    # Create checkpoint for this turn (user prompt → workspace snapshot)
    if user_text and user_text.strip():
        run_lupe("prompt", user_text.strip(), "--agent", agent)

    # Attach agent response to that checkpoint
    run_lupe("respond", assistant_text.strip())

    sys.exit(0)


if __name__ == "__main__":
    main()
