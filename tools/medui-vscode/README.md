# MEDUI Language Support for VS Code

## Installation

### Local Installation
1. Copy the `tools/medui-vscode` directory to `~/.vscode/extensions/`.
2. Restart Visual Studio Code.

### Using vsce
1. Install the `vsce` package globally if you haven't already:
   ```bash
   npm install -g vsce
   ```
2. Package the extension:
   ```bash
   cd tools/medui-vscode
   vsce package
   ```
3. Install the packaged extension in VS Code:
   ```bash
   code --install-extension medui-0.0.1.vsix
   ```

## Usage

Open a `.medui` file in Visual Studio Code to enjoy syntax highlighting.
