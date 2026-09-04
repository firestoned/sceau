# sceau documentation

MkDocs Material site for [sceau](https://github.com/firestoned/sceau),
published to <https://firestoned.github.io/sceau/>.

Build and serve from the repository root:

```sh
make docs         # regenerate CALM diagrams + build into docs/site/
make docs-serve   # live-reload server at http://127.0.0.1:8000
```

Sources live in `src/`; the configuration is `mkdocs.yml`; Python
dependencies are managed with Poetry (`pyproject.toml`).
