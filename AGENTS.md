# Agent Skills (OpenCode)

This project uses skills installed under `.agents/skills/` (or a compatible path).

## Core Rules

- If a task matches a skill, invoke it with the `skill` tool before acting.
- Skills are located in `.agents/skills/<skill-name>/SKILL.md`.
- Follow the skill workflow strictly; do not partially apply it.
- Never skip required steps such as spec, plan, or test when a skill demands them.

## Intent -> Skill Mapping

Map the user's intent to the matching skill automatically:

- Feature / new functionality -> `spec-driven-development`, then `incremental-implementation` and `test-driven-development`
- Planning / breakdown -> `planning-and-task-breakdown`
- Bug / failure / unexpected behavior -> `debugging-and-error-recovery`
- Code review -> `code-review-and-quality`
- Refactoring / simplification -> `code-simplification`
- API or interface design -> `api-and-interface-design`
- UI work -> `frontend-ui-engineering`

## Execution Model

For every request:

1. Determine if any skill applies (even a small chance).
2. Load the skill with `skill({ name: "<skill-name>" })`.
3. Follow the skill workflow exactly.
4. Only proceed to implementation once required steps are complete.

## Code Style

Formatting and naming follow the language toolchain:

- Rust: `rustfmt`; 4 spaces; functions and variables snake_case; types
  PascalCase; max width 100.
- Markdown, JSON, YAML: Prettier where applicable.

All languages:

- Use `//` comments in code; explain intent, not restate the code.
- No emoji.
- Keep files under 800 lines; split by module when larger.
- Comments may be Chinese for domain intent; keep identifiers English.

## Commands

- Just runner: `just` (list recipes), `just check` (full gate), `just fmt`, `just test`
- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Format check: `cargo fmt --all --check`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Full check: `pwsh ./scripts/check.ps1` (PowerShell 7+) or
  `powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1`
  (Windows PowerShell), or `bash ./scripts/check.sh` (Linux)

## Boundaries

- Do not commit Minecraft assets, resource packs, or private `.litematic`
  files.
- Do not copy code or assets from the unlicensed reference repository.
- Keep public protocols versioned; do not leak internal buffers into DTOs.
- Update `tasks/todo.md` as work progresses.
