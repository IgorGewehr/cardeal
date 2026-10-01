// Carrega o WASM (já pré-carregado pelo <link rel=preload>) e liga o app ao canvas.
import init, { iniciar } from "./cardeal_web.js";

await init();
await iniciar();
document.getElementById("carregando")?.remove();
