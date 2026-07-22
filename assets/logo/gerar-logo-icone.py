#!/usr/bin/env python3
# Caminho relativo: assets/logo/gerar-logo-icone.py
#
# Gera o asset binário embutido no binário do agentry (MT-111):
# `crates/cli/assets/logo-icone.rgb`, consumido por `crates/cli/src/tui/logo.rs`
# via `include_bytes!`.
#
# Ferramenta de pré-processamento OFFLINE -- não roda como parte do
# `cargo build`, nem depende de Pillow em runtime. Só precisa ser rodada de
# novo se `agentry-logo-fonte.png` mudar (novo recorte, nova resolução).
# Requer Pillow (`pip install pillow`), só nesta máquina de desenvolvimento.
#
# Técnica: recorta só o ícone (chapéu + robô + terminal, sem o texto
# "AGENTRY"/subtítulo do arquivo original, que ficaria ilegível nesta
# resolução -- o texto é renderizado à parte, como texto de terminal de
# verdade, nítido) e reduz para uma grade de LARGURA x (ALTURA*2) pixels --
# cada linha de terminal representa duas linhas de pixel (renderizado como
# "▀" com cor de primeiro plano = pixel de cima, cor de fundo = pixel de
# baixo -- mesma técnica de ferramentas como `chafa`/`viu`).
#
# Uso: python3 gerar-logo-icone.py [largura_em_colunas]

import sys
from pathlib import Path
from PIL import Image

RAIZ = Path(__file__).resolve().parent
FONTE = RAIZ / "agentry-logo-fonte.png"
DESTINO = RAIZ.parent.parent / "crates" / "cli" / "assets" / "logo-icone.rgb"

# Recorte manual (left, top, right, bottom) -- só o ícone, achado inspecionando
# as faixas de conteúdo não-preto da imagem original (chapéu+robô+terminal
# ficam entre as linhas ~300-816 da imagem fonte de 1254x1254).
CROP = (280, 295, 1055, 820)


def gerar(largura_colunas: int) -> None:
    img = Image.open(FONTE).convert("RGB").crop(CROP)
    w, h = img.size
    linhas_terminal = round(largura_colunas * (h / w) / 2)
    altura_px = linhas_terminal * 2
    resized = img.resize((largura_colunas, altura_px), Image.LANCZOS)
    DESTINO.write_bytes(resized.tobytes())
    print(
        f"gerado: {DESTINO} ({largura_colunas}x{altura_px} px, "
        f"{linhas_terminal} linhas de terminal, {largura_colunas * altura_px * 3} bytes)"
    )
    print(
        f"lembrar de atualizar LARGURA={largura_colunas} e ALTURA={altura_px} "
        f"em crates/cli/src/tui/logo.rs se mudou"
    )


if __name__ == "__main__":
    cols = int(sys.argv[1]) if len(sys.argv) > 1 else 44
    gerar(cols)
