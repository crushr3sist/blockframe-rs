# Contributing to BlockFrame

Thanks for considering contributing! BlockFrame is an open project and contributions are welcome.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/blockframe-rs.git`
3. Create a branch: `git checkout -b your-feature-name`
4. Make your changes
5. Test thoroughly
6. Submit a pull request

## Development Setup

**Requirements:**

- Rust stable toolchain
- WinFSP (Windows) or FUSE dev libraries (Linux)

**Build:**

```bash
cargo build
```

**Run tests:**

```bash
cargo test
```

## Guidelines

**Code:**

- Follow existing code style and patterns
- Add tests for new functionality
- Keep commits focused and atomic
- Run `cargo fmt` before committing

**Pull Requests:**

- Describe what your PR does and why
- Reference any related issues
- Make sure tests pass
- Keep PRs focused on a single concern

**Documentation:**

- Update README.md if you change user-facing behavior
- Add module-level docs for new components
- Keep comments concise and technical

## Areas for Contribution

- Please refer to the [roadmap](https://github.com/crushr3sist/blockframe-rs?tab=readme-ov-file#roadmap) for future features
- Bug fixes and error handling improvements
- Performance optimizations
- Platform-specific fixes (Windows/Linux filesystem behavior)
- Additional test coverage
- Documentation improvements

## Questions?

Open an issue for discussion before starting major refactoring or new features. This helps avoid duplicate work and ensures your contribution aligns with the project direction.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
