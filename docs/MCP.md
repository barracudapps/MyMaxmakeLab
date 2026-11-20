# MyMaxmakeLab for AI assistants (MCP)

`mymaxmakelab-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server shipped with the application. An assistant connected to it can draw
boxes, mazes, puzzles, ornaments and text, turn SVG into G-code, carve a 3D
model, and analyse or repair any G-code file, with the same code as the
application.

It **never moves a machine**: programs are written to files that you open in
MyMaxmakeLab to preview, frame and start with every safety check.

## Where it is

| System | Path |
|---|---|
| macOS | `/Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp` |
| Linux | next to `mymaxmakelab`, e.g. `~/.local/bin/mymaxmakelab-mcp` |
| Windows | next to `mymaxmakelab.exe`, `mymaxmakelab-mcp.exe` |

It updates with the application.

## Tools

| Tool | Does |
|---|---|
| `list_machine_presets`, `list_fonts` | what is available |
| `generate_box` | finger-jointed box panels for an object (inside size, thickness, lid, kerf) |
| `generate_maze` | maze walls or corridors, optional solution |
| `generate_jigsaw` | jigsaw puzzle cut lines |
| `generate_ornament` | Viking, Celtic, African, Maya, Greek, Japanese, Art Deco, Polynesian, Islamic friezes or tilings |
| `text_to_svg` | text as outlines, bundled or single-line fonts |
| `svg_to_gcode` | laser line or fill, CNC profile or pocket, for a machine preset |
| `analyze_gcode` | duration, size, lengths, Z range, warnings |
| `optimize_gcode` | same path, fewer lines |
| `stl_relief_to_gcode` | 3D relief from an STL or OBJ model |

Results are returned inline, or written to `output_path` (an existing file is
only replaced with `overwrite: true`).

## Set it up

The examples use the macOS path; replace it with yours.

**Claude Code**

```bash
claude mcp add mymaxmakelab -- /Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp
```

**Claude Desktop**: `~/Library/Application Support/Claude/claude_desktop_config.json`
(Windows: `%APPDATA%\Claude\claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "mymaxmakelab": {
      "command": "/Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp"
    }
  }
}
```

**OpenAI Codex CLI** (GPT models): `~/.codex/config.toml`

```toml
[mcp_servers.mymaxmakelab]
command = "/Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp"
```

**Gemini CLI**: `~/.gemini/settings.json`

```json
{
  "mcpServers": {
    "mymaxmakelab": {
      "command": "/Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp"
    }
  }
}
```

**Cursor** (`~/.cursor/mcp.json`, same `mcpServers` block as above) and
**VS Code** (`.vscode/mcp.json`):

```json
{
  "servers": {
    "mymaxmakelab": {
      "type": "stdio",
      "command": "/Applications/MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab-mcp"
    }
  }
}
```

**DeepSeek, Grok, Mistral and other models**: use an MCP client that lets you
pick the model (for example LibreChat, Cherry Studio, Continue or LM Studio)
and add the server as a local *stdio* command with the path above.

**ChatGPT and Le Chat apps** only accept remote MCP servers reachable over
HTTPS; this server is local by design (it reads and writes your files). Use
Codex CLI, or a client from the list above.

## Example

> Make a box for a 70 × 45 × 30 mm Raspberry Pi in 3 mm plywood with a lid,
> save it to ~/Desktop/pi-box.svg, then turn it into laser cutting G-code for
> the GRBL laser at 80 % and 400 mm/min in ~/Desktop/pi-box.nc.
