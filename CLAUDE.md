The documentation in this repo, starting with the README.md, is the source of truth. If the documentation and the implementation differ, that's a bug, and must be addressed.

Each source directory in this repo must have a README.md that describes the interfaces and functions of the components of that directory and its subdirectories.

When looking for APIs or an understanding of how something works, always start with the documentation.

If you are making more than a trivial change, always use the /orchestrate skill, to make sure that tests are run and the implementation is verified.

Prefer end-to-end and integration tests over mocks. The purpose of this application is to communicate with a Neovim process, so the tests should actually start a Neovim process and communicate with it.

## Markdown

When editing Markdown files, avoid adding hard line breaks in a single paragraph.

After editing a Markdown file, reformat it using `prettier --write`.
