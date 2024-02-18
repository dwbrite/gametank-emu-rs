import init, { initialize, play } from "./bin/gametank-emu-rs.js";

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function main() {
    await init();

    let canvas = document.body.appendChild(document.createElement("canvas"));
    canvas.setAttribute('class', 'canvas');
    canvas.width = 256;
    canvas.height = 256;

    play(canvas);
}

main();
