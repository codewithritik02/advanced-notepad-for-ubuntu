# Contributing to Personal Notepad

Thank you for helping. Everyone is welcome: code, bug reports, ideas, documentation, testing and Windows support.

## Report a bug or suggest an idea
Open an issue. Say what you did, what you expected and what happened. Add your OS and the app version (Settings → About). Screenshots help.

## Send a change
1. Fork the repository and create a branch.
2. Keep the change focused on one thing.
3. Run the checks and make sure they pass:
   ```bash
   npm run typecheck
   npm run lint
   npm test
   npm run test:safety
   npm run test:rust
   npm run lint:rust
   npm run format:rust:check
   ```
4. Open a pull request. Explain what changed and why. Add screenshots for interface changes.

## Rules
- **Never test against real notes.** Run the app and tests with a throw-away data folder. See "Use a separate data folder while developing" in the README.
- **No network, tracking or accounts.** The app is private and offline by design.
- Add or update tests for behavior you change.

## License
By contributing, you agree that your contribution is licensed under the [MIT License](LICENSE).
