#!/usr/bin/env python3
"""
Lupe stop hook — auto-captures every user message and agent response.
Fires after each agent turn. Creates a checkpoint for the turn and saves the response.
Works with Claude Code, Codex CLI, and Cursor.
"""
import json
import os
import shutil
import subprocess
import sys

LUPE = (
    os.environ.get("LUPE_BIN")
    or shutil.which("lupe")
    or os.path.expanduser("~/.cargo/bin/lupe")
)


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

                    candidate = msg.get("model") or entry.get("model")
                    if candidate:
                        model = candidate

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
    agent_model = (
        os.environ.get("LUPE_AGENT_MODEL")
        or model
        or payload.get("model")
        or "unknown"
    )
    return f"{agent_name}/{agent_model}"


SENSITIVE_KEYWORDS = [
    "secret", "password", "token", "api key", "api_key", "credential",
    "vulnerability", "exploit", "cve", "private key", "private_key",
    "certificate", ".env", "don't log", "keep this private", "sensitive",
    "confidential",
]



def is_sensitive(text: str) -> bool:
    if not text:
        return False
    lower = text.lower()
    return any(k in lower for k in SENSITIVE_KEYWORDS)


def read_lupeprivate(workspace: str) -> list[str]:
    path = os.path.join(workspace, ".lupeprivate")
    if not os.path.exists(path):
        return []
    try:
        with open(path) as f:
            return [
                l.strip() for l in f
                if l.strip() and not l.strip().startswith("#")
            ]
    except Exception:
        return []


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        sys.exit(0)

    # Only run in projects that have opted in to lupe
    if not os.path.isdir(os.path.join(os.getcwd(), ".lupe")):
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

    # Check if this turn should be private
    workspace = os.getcwd()
    extra_keywords = read_lupeprivate(workspace)
    all_keywords = SENSITIVE_KEYWORDS + extra_keywords

    def is_sensitive_custom(text: str) -> bool:
        if not text:
            return False
        lower = text.lower()
        return any(k.lower() in lower for k in all_keywords)

    private = is_sensitive_custom(user_text or "") or is_sensitive_custom(assistant_text or "")

    # Create checkpoint for this turn (user prompt → workspace snapshot)
    session_id = payload.get("session_id")
    if user_text and user_text.strip():
        args = ["prompt", user_text.strip(), "--agent", agent]
        if session_id:
            args += ["--session", session_id]
        if private:
            args.append("--private")
        run_lupe(*args)
    elif private:
        # No user text but response is sensitive — mark latest checkpoint private
        run_lupe("private")

    # Attach agent response to that checkpoint
    run_lupe("respond", assistant_text.strip())

    # Backfill the model onto the latest checkpoint — fixes checkpoints created by
    # `lupe save` which only know the agent name, not the model.
    run_lupe("set-agent", agent)

    # If private flag wasn't set at prompt creation, mark now in case response triggered it
    if private:
        run_lupe("private")

    sys.exit(0)


if __name__ == "__main__":
    main()
