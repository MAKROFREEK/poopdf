## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (uses the gh CLI). See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: GLOSSARY.md at the repo root + docs/adr/. See `docs/agents/domain.md`.

### Code Style & Architecture Guidelines
1. **Human Readability First:** Write code as if a human junior developer needs to read, maintain, and debug it tomorrow. Avoid overly condensed shorthand, ternary pyramids, or dense meta-programming.
2. **No Unrequested Abstractions:** Do not create abstract base classes, generic factories, or indirection layers unless explicitly requested. Write straightforward, concrete implementations.
3. **Error Handling:** Use explicit, idiomatic error handling (e.g., standard try/catch or Result types). Never use silent failure blocks or catch-alls that swallow stack traces.
4. **Commenting:** Explain the *why*, not the *what*. Do not add robotic line-by-line comments for self-evident code (e.g., `// increment i by 1`), but do document non-obvious business logic or architectural workarounds.
