<!-- SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

> **Mirror:** This repo is a read-only mirror of [gitlab.com/vauchi/tui](https://gitlab.com/vauchi/tui). Please open issues and merge requests there.

[![Pipeline](https://img.shields.io/endpoint?url=https://vauchi.gitlab.io/tui/badges/pipeline.json&label=pipeline)](https://gitlab.com/vauchi/tui)
[![Coverage](https://img.shields.io/endpoint?url=https://vauchi.gitlab.io/tui/badges/coverage.json&label=coverage)](https://gitlab.com/vauchi/tui)
[![REUSE](https://api.reuse.software/badge/gitlab.com/vauchi/tui)](https://api.reuse.software/info/gitlab.com/vauchi/tui)

> [!NOTE]
> **You're early — and that's the point.** Vauchi is pre-alpha and
> under heavy development: not yet ready for production, and APIs may
> change without notice. If you're here now, you can help shape it —
> try it, break it, and tell us what's missing.

# Vauchi TUI Display

The terminal display library of Vauchi — living contact cards, exchanged
in person.

A library, built on Ratatui and Crossterm: it renders Core's generic
presentation commands in a terminal and reports key input as generic
events. Its only Vauchi dependency is `vauchi-contract`; it links no
Core. The `vauchi-tui` binary wires it to Core and is built and shipped
by [linux-app](https://gitlab.com/vauchi/linux-app).

Core prepares every command — copy, state, tokens, identifiers — and
interprets every event; this library carries no Vauchi domain
vocabulary.

## Build

```bash
cargo build
cargo test
```

## Mandatory Development Rules

**TDD**: Red→Green→Refactor. Test FIRST or delete code and restart.

**Structure**: `src/` = production code only. `tests/` = tests only. Siblings, not nested.

## Support the Project

Vauchi is open source and community-funded — no VC money, no data harvesting.

- [GitHub Sponsors](https://github.com/sponsors/vauchi)
- [Liberapay](https://liberapay.com/Vauchi/donate)
- [Supporters](https://vauchi.app/docs/about/supporters/) for sponsorship tiers

## License

GPL-3.0-or-later
