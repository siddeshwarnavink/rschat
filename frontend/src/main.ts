import * as utils from "./utils";
import * as ws from "./wasm/crypto_wasm.js"

const welcome_dialog = document.getElementById("welcome_dialog") as HTMLDialogElement | null;
const messages = document.getElementById("messages");
const message_form = document.getElementById("message_form") as HTMLFormElement | null;

interface User {
    id: string;
    name: string;
    public_key: string;
    private_key: string;
};

// Global state
let socket: WebSocket | null = null;
let my_name: string | null = null;
let my_keys: { public_key: string; private_key: string; } | null = null;
const users: { [id: string]: User } = {};

function append_user_message(name: string, text: string): void {
    const color = utils.name_color(name);
    if (messages) {
        messages.innerHTML += `
        <div class="user-message">
            <b style="color:${color}">${name}: </b>
            ${text}
        </div>
        `;
    }
}

function append_server_message(text: string): void {
    if (messages) {
        messages.innerHTML += `
            <div class="server-message">${text}</div>
        `;
    }
}

function on_message(event: MessageEvent): void {
    const msg = JSON.parse(event.data);
    console.log(msg);

    switch (msg.kind) {
        case "new_user":
            users[msg.user.id] = msg.user;
            append_server_message(`${msg.user.name} joined the chat.`);
            break;
        case "relay_message":
            const user = users[msg.sender];
            if (my_keys && my_keys) {
                const text = ws.decrypt_message(msg.payload, my_keys.private_key);
                append_user_message(user.name, text);
            }
            break;
        case "user_left":
            const name = users[msg.user_id].name;
            delete users[msg.user_id];
            append_server_message(`${name} left the chat.`);
            break;
    }
}

function on_welcome(event: SubmitEvent) {
    if (event.target == null) return;
    const welcome_form = event.target as HTMLFormElement;

    my_name = welcome_form.getElementsByTagName("input")[0].value;
    socket = new WebSocket("/ws");

    socket.onopen = function() {
        if (socket == null) return;
        if (my_keys == null) return;
        if (message_form == null) return;

        socket.send(JSON.stringify({
            kind: "first",
            public_key: my_keys.public_key,
            name: my_name
        }));

        message_form.addEventListener("submit", (event: SubmitEvent) => {
            event.preventDefault();

            if (event.target == null) return;
            if (my_name == null) return;

            const text = message_form.getElementsByTagName("input")[0].value;
            append_user_message(my_name, text);

            Object.keys(users).forEach(user_id => {
                const user = users[user_id];
                const payload = ws.encrypt_message(text, user.public_key);
                if (socket) {
                    socket.send(JSON.stringify({
                        kind: "send_message",
                        recipient: user_id,
                        payload
                    }));
                }
            });

            (event.target as HTMLFormElement).reset();
        });
    };

    socket.onmessage = on_message;

    append_server_message(`${my_name} joined the chat.`);
}

my_keys = JSON.parse(ws.generate_keypair());

if (welcome_dialog) {
    const form = welcome_dialog.getElementsByTagName("form")[0];
    form.addEventListener("submit", on_welcome);
    welcome_dialog.showModal();
}
