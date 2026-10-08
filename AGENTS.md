# Project instructions

## Product and scope

- Use `SubtitleVocabularyList` as the product name, public repository name and local workspace directory name.
- Follow the confirmed 0.1 requirements in `docs/requirements/PRD.md`. Do not add deferred features or turn open questions into committed requirements.
- Follow the confirmed technology decisions in `docs/design/architecture.md`. Keep proposed and verified implementation details distinct.
- Preserve unrelated changes. Deliver the simplest complete solution within the user's authorization.

## Documentation

- Read `docs/README.md` before creating or changing project documents.
- Keep product requirements and acceptance criteria in the PRD. Keep the technology stack, module design, interfaces and data design in architecture or detailed design documents.
- Give functional and nonfunctional requirements stable identifiers. Reference these identifiers in designs and acceptance evidence.
- When behavior changes, update the affected requirements, designs and acceptance criteria together. Do not describe unimplemented or untested behavior as complete.
- Use Chinese for project documentation unless the user requests otherwise. Use standard document sections and explicit review status.

## Implementation and validation

- All collection inputs must use the same vocabulary merge and persistence rules. Preserve examples, original audio and learning history.
- Core learning and storage must work offline. Network access is limited to configured integrations and user-authorized online actions.
- Reuse Pot code selectively after reviewing its dependencies, failure behavior and license. Do not make the application depend on a running Pot installation.
- Validate changes in proportion to their impact. Test actual capture, audio and synchronization flows when those capabilities are implemented.
- During feature development, use the local development environment and proportional local verification. Do not rebuild or install release packages for each feature.
- Build installers, standalone release directories, source materials and perform installation acceptance only when the user explicitly requests packaging or a version is being released. Keep packaging a manual entry point.
- Use `pnpm desktop:dev` for local debugging, `pnpm desktop:build` for a manually requested local debug executable, and `pnpm desktop:package` for a manually requested release package.
- Keep runtime user data, media, model weights and credentials out of the public repository. Use synthetic fixtures for committed tests.
- Clean up disposable temporary artifacts. Keep necessary deliverables in durable project locations.

## Git delivery

- After validation, commit only intended changes and push the current branch unless the user instructs otherwise.
- Do not create additional branches or pull requests without a request.
- Report the actual validation, commit and push result. Keep local verification distinct from deployed integration results.
