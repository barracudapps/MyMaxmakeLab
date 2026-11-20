<sub>[← Production, optimisation et coûts](11-production-statistiques.md) · [Sommaire](README.md) · [Dépannage et questions fréquentes →](13-depannage.md)</sub>

# 12. Assistants IA (MCP)

Le serveur **MCP** fourni avec l'application permet à Claude, Codex (GPT),
Gemini ou à tout client MCP de créer boîtes, labyrinthes, puzzles, motifs et
textes, de transformer un SVG en G-code, de creuser un relief 3D et
d'analyser ou réparer un programme. Il **ne déplace jamais la machine** :
les programmes sont écrits dans des fichiers que vous ouvrez dans
MyMaxmakeLab pour les vérifier, cadrer et lancer.

```mermaid
sequenceDiagram
    participant U as Vous
    participant A as Assistant IA
    participant S as Serveur MCP
    participant M as MyMaxmakeLab
    U->>A: « Une boîte pour un objet de 70 × 45 × 30 mm »
    A->>S: generate_box, svg_to_gcode
    S-->>A: pi-box.svg, pi-box.nc (durée, dimensions)
    A-->>U: fichiers prêts
    U->>M: ouvrir, prévisualiser, cadrer, démarrer
```

**Aide › Assistants IA (MCP)** copie le chemin du serveur. La mise en place
pour chaque assistant (Claude Code, Claude Desktop, Codex, Gemini CLI,
Cursor, VS Code…) est décrite dans [MCP.md](../MCP.md).

---

<sub>[← Production, optimisation et coûts](11-production-statistiques.md) · [Sommaire](README.md) · [Dépannage et questions fréquentes →](13-depannage.md)</sub>
