const socket = new WebSocket("ws://localhost:3333");
const app = document.getElementById("app");

socket.onopen = function(event) {
  console.log("onopen", event);
};

socket.onmessage = function(event) {
  console.log("onmessage", event);
  app.innerHTML = event.data;
};

socket.onclose = function(event) {
  console.log("onclose", event);
};

