# Rasteriza a marca do Lunno (mesma geometria de assets/marca/lunno-icone.svg) com PIL,
# desenhando a 8x e reduzindo com LANCZOS para bordas suaves.
import sys
from PIL import Image, ImageDraw, ImageFont

MARCA = (0x5B, 0x45, 0xE0, 255)
BRANCO = (255, 255, 255, 255)
destino = sys.argv[1]


def icone(lado):
    k = 8
    s = lado * k / 200.0
    img = Image.new("RGBA", (lado * k, lado * k), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, lado * k - 1, lado * k - 1], radius=44 * s, fill=MARCA)
    # lua: disco claro menos disco deslocado
    lua = Image.new("L", img.size, 0)
    dl = ImageDraw.Draw(lua)
    cx, cy, r = 96, 104, 58
    dl.ellipse([(cx - r) * s, (cy - r) * s, (cx + r) * s, (cy + r) * s], fill=255)
    cx, cy, r = 124, 80, 50
    dl.ellipse([(cx - r) * s, (cy - r) * s, (cx + r) * s, (cy + r) * s], fill=0)
    img.paste(Image.new("RGBA", img.size, BRANCO), (0, 0), lua)
    estrela = [(138, 66), (142.5, 77.5), (154, 82), (142.5, 86.5), (138, 98), (133.5, 86.5), (122, 82), (133.5, 77.5)]
    d.polygon([(x * s, y * s) for x, y in estrela], fill=BRANCO)
    return img.resize((lado, lado), Image.LANCZOS)


for lado in (256, 512):
    icone(lado).save(f"{destino}/lunno-icone-{lado}.png")

# logo horizontal: marca + "Lunno"
alt = 160
logo = Image.new("RGBA", (alt * 4, alt), (0, 0, 0, 0))
logo.paste(icone(alt), (0, 0))
try:
    fonte = ImageFont.truetype(sys.argv[2], int(alt * 0.62))
except Exception:
    fonte = ImageFont.load_default()
d = ImageDraw.Draw(logo)
d.text((alt + alt * 0.18, alt * 0.5), "Lunno", font=fonte, fill=(0x11, 0x11, 0x14, 255), anchor="lm")
caixa = logo.getbbox()
logo.crop((0, 0, caixa[2] + 8, alt)).save(f"{destino}/lunno-logo-horizontal.png")
print("ok")
