# Project Development Rules

These rules apply to all development work in this repository.

## Git Commits

- After completing a feature and passing the relevant checks, commit its changes to Git.
- Each commit must contain only changes related to that feature. Preserve existing unrelated uncommitted changes and keep them out of the commit.

## Versioning and Builds

- When a build is required, bump the version before building and keep all version declarations and corresponding lockfiles consistent throughout the project.

## Code and Documentation Content

- Never include real media titles, authors' names, or similar identifying information in code. Tests, examples, and fixtures must also use fictional or anonymized content.
- Do not add prompt text, developer handoff notes, or other content unrelated to the actual project to code or documentation.

## User Interface

- Before modifying the user interface, load and read the `design-taste-frontend` skill installed in the current environment and follow the requirements that apply to the change.
- The `design-taste-frontend` skill comes from [leonxlnx/taste-skill](https://github.com/leonxlnx/taste-skill).
- Prefer shadcn/ui components and reuse existing project components. Add custom implementations only when existing components cannot meet the requirements.
