# Lupe Agent Instructions

Run `lupe docs` for full command reference.

## Quick Start

Before starting ANY task, run:
```
lupe docs
```

This gives you the complete reference for all commands, workflows, and the merge guide.

## Core Rule

**Always use lupe to track your work.** Never manually revert files — use `lupe restore`.

Check `.lupeprivate` if it exists. If your task matches a pattern there, run `lupe private` immediately before doing anything else. Also run `lupe private` for anything involving secrets, credentials, API keys, vulnerabilities, or anything the user marks sensitive.
