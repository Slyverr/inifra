#!/usr/bin/env python3
"""A tiny OpenAI-compatible Chat Completions server for the TUI smoke runner.

Behavior depends on the last user message: containing FAIL returns HTTP 500, SLOW waits
six seconds before the first token, and anything else answers after a short pause.
Usage: mock_provider.py PORT
"""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ANSWER = (
    "Inifra reads the catalog, validates each input against the template schema, and asks for "
    "anything missing before it generates files.\n\n"
    "- Pick a template\n- Fill the inputs\n- Review the plan\n\n"
    "Nothing is written until you approve the plan."
)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _json(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self._json(200, {"data": [{"id": "mock-1"}, {"id": "mock-2"}]})

    def do_POST(self):
        try:
            self._post()
        except (BrokenPipeError, ConnectionResetError):
            pass  # the client cancelled mid-stream

    def _post(self):
        length = int(self.headers.get("content-length", 0))
        try:
            request = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            request = {}
        users = [m.get("content", "") for m in request.get("messages", []) if m.get("role") == "user"]
        last = users[-1] if users else ""
        if "FAIL" in str(last):
            self._json(500, {"error": {"message": "mock provider failure"}})
            return
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        time.sleep(6 if "SLOW" in str(last) else 0.4)
        for word in ANSWER.split(" "):
            chunk = {"choices": [{"delta": {"content": word + " "}}]}
            self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
            self.wfile.flush()
            time.sleep(0.02)
        done = {
            "choices": [{"delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 40, "total_tokens": 50},
        }
        self.wfile.write(f"data: {json.dumps(done)}\n\ndata: [DONE]\n\n".encode())
        self.wfile.flush()


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
