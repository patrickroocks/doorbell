# Technical Documentation

This directory contains the technical documentation of the project in different `*.md` files. Diagrams are stored next to them as `*.drawio.svg` files and embedded into the Markdown.


## Recommended VS Code extensions

| Extension                                                                                             | Used for                                                                               |
| ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| [Markdown All in One](https://marketplace.visualstudio.com/items?itemName=yzhang.markdown-all-in-one) | Editing the Markdown files: formatting tables (`Shift+Alt+F`), shortcuts, live preview |
| [Draw.io Integration](https://marketplace.visualstudio.com/items?itemName=hediet.vscode-drawio)       | Editing the `*.drawio.svg` diagrams directly in VS Code                                |


## Diagrams with draw.io

A `*.drawio.svg` file is a regular SVG image that also contains the editable draw.io diagram. So the same file is displayed as an image by GitHub, the Markdown preview and the PDF export, and can be edited with draw.io at any time - there is no separate source file and no export step.

* Edit a diagram: open the `*.drawio.svg` file in VS Code. The Draw.io Integration extension opens it in the draw.io editor; on save, the SVG image is updated as well.
* Add a new diagram:
  1. Create an empty file `doc/<name>.drawio.svg` and open it in VS Code - the draw.io editor starts with an empty canvas.
  2. Draw the diagram and save.
  3. Embed it into the Markdown file: `![](<name>.drawio.svg)`

Outside of VS Code, the files can also be opened with the draw.io desktop app or on [app.diagrams.net](https://app.diagrams.net). Don't edit the SVG by hand or with other SVG editors, that would get the image and the embedded diagram out of sync.


## PDF export

The documents are also published as PDFs on the project website. To create them, run

```bash
doc/scripts/render-doc.sh
```

This renders the Markdown files in GitHub style to `project-doc/technical-docu-output/<name>.pdf` and requires `pandoc` and Chrome/Chromium (plus internet access for the GitHub stylesheet). On Ubuntu:

```bash
sudo apt install pandoc chromium-browser
```

On Ubuntu, `chromium-browser` installs the Chromium snap. If Google Chrome is already installed, `pandoc` alone is enough; the script uses whichever of `google-chrome`, `chromium` or `chromium-browser` it finds first.

Each document has its own script `doc/scripts/render-<name>-pdf.sh`, which calls the shared helper [`scripts/helper/render-md-pdf.sh`](scripts/helper/render-md-pdf.sh). For a new document, add such a script and call it from [`render-doc.sh`](scripts/render-doc.sh).

`render-doc.sh` also calls [`render-all-drawio-svgs.sh`](scripts/render-all-drawio-svgs.sh), which renders every `doc/*.drawio.svg` to `project-doc/technical-docu-output/<name>.png` (original size) and `<name>@2x.png` (high-res, twice the size). New diagrams are picked up automatically.
