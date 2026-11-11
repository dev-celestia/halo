#!/usr/bin/python3
"""Network-free Pi 1.4.0 protocol fixture; never invokes a model or real Pi."""
import json
import os
import re
import signal
import subprocess
import sys
import time

case = os.environ.get("PI_FAKE_CASE", "immediate")
if "--version" in sys.argv:
    if case == "version_timeout":
        time.sleep(60)
    if case == "version_oversize":
        print("x" * 20000)
    else:
        print(os.environ.get("PI_FAKE_VERSION", "1.4.0"))
    sys.exit(0)

record = os.environ.get("PI_FAKE_RECORD")
def record_value(value):
    if record:
        with open(record, "a") as f:
            f.write(json.dumps(value) + "\n")

record_value({"argv": sys.argv[1:], "openai_present": "OPENAI_API_KEY" in os.environ,
              "openai": os.environ.get("OPENAI_API_KEY") if os.environ.get("OPENAI_API_KEY", "").startswith("test-") else None})

def emit(value, split=False):
    raw = json.dumps(value, ensure_ascii=False).encode() + b"\r\n"
    if split:
        for byte in raw:
            os.write(1, bytes([byte]))
    else:
        sys.stdout.buffer.write(raw)
        sys.stdout.buffer.flush()

def response(command, success=True, data=None, error=None):
    emit({"type": "response", "id": command["id"], "command": command["type"],
          "success": success, "data": data, "error": error})

def settled():
    emit({"type": "agent_end", "messages": [], "willRetry": False})
    emit({"type": "agent_settled", "aborted": False})

model = {"provider": "fake", "id": "model"}
if "-e" in sys.argv and case != "no_ready":
    extension = sys.argv[sys.argv.index("-e") + 1]
    with open(extension) as f:
        nonce = re.search(r'setStatus\("halo-approval-v1", "([^"]+)"\)', f.read()).group(1)
    if case == "extension_error":
        emit({"type": "extension_error", "event": "session_start"})
    else:
        emit({"type": "extension_ui_request", "id": "ready", "method": "setStatus",
              "statusKey": "halo-approval-v1", "statusText": nonce})

for line in sys.stdin.buffer:
    command = json.loads(line)
    record_value(command)
    kind = command["type"]
    if kind == "get_state":
        response(command, data={"model": model, "sessionFile": "/tmp/fake pi session.jsonl", "sessionId": "fake-session-id",
                                "isStreaming": False, "isCompacting": False})
    elif kind == "get_available_models":
        if case == "unsupported_models":
            response(command, success=False, error="get_available_models unavailable")
            continue
        if case == "blocked":
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            child = subprocess.Popen([sys.executable, "-c", "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(60)"])
            record_value({"descendant": child.pid, "leader": os.getpid()})
        response(command, data={"models": [
            model,
            {"provider": "fake", "id": "other"},
        ]})
        if case == "blocked":
            time.sleep(60)
    elif kind == "set_model":
        model = {"provider": command["provider"], "id": command["modelId"]}
        response(command, data=model)
    elif kind == "prompt":
        if case == "reject":
            response(command, success=False, error="fixture rejected prompt")
        elif case == "handled":
            response(command, data={"disposition": "handled"})
        elif case == "malformed":
            os.write(1, b"{RAW_PROMPT_SECRET not json}\n")
        elif case == "oversize":
            os.write(1, b"x" * (4 * 1024 * 1024 + 10))
        elif case == "invalid_utf8":
            os.write(1, b'{"type":"error","message":"\xff"}\n')
        elif case == "partial_eof":
            os.write(1, b'{"type":"agent_settled"')
            sys.exit(0)
        elif case == "no_ack":
            pass
        elif case in ("crash", "eof"):
            response(command, data={"disposition": "started"})
            sys.exit(7 if case == "crash" else 0)
        elif case in ("wait", "steer_reject"):
            emit({"type": "agent_start"})
            response(command, data={"disposition": "started"})
        elif case.startswith("approval"):
            response(command, data={"disposition": "started"})
            emit({"type": "agent_start"})
            emit({"type": "extension_ui_request", "method": "confirm", "id": "approval-" + str(os.getpid()),
                  "title": "Allow tool execution?", "message": "write\nfixture.txt", "timeout": 150 if case == "approval_timeout" else 60000})
        elif case in ("retry", "provider_error"):
            response(command, data={"disposition": "started"})
            emit({"type": "agent_start"})
            emit({"type": "message_start", "message": {"role": "assistant", "content": []}})
            emit({"type": "message_end", "message": {"role": "assistant", "content": [], "stopReason": "error", "errorMessage": "fixture provider failure"}})
            emit({"type": "agent_end", "messages": [], "willRetry": case == "retry"})
            time.sleep(0.08)
            if case == "retry":
                emit({"type": "agent_start"})
                emit({"type": "message_start", "message": {"role": "assistant", "content": []}})
                emit({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "text", "text": "recovered"}], "stopReason": "stop"}})
            emit({"type": "agent_settled", "aborted": False})
        else:
            # Events, including settlement, deliberately precede the acceptance response.
            emit({"type": "agent_start"})
            emit({"type": "message_start", "message": {"role": "assistant", "content": []}})
            emit({"type": "message_update", "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "reasoning"}})
            emit({"type": "message_update", "assistantMessageEvent": {"type": "thinking_end", "contentIndex": 0, "content": "reasoning"}})
            emit({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "你好\u2028\u2029draft"}}, split=True)
            emit({"type": "message_update", "assistantMessageEvent": {"type": "text_end", "contentIndex": 1, "content": "你好\u2028\u2029corrected"}})
            emit({"type": "message_end", "message": {"role": "assistant", "stopReason": "stop", "content": [
                {"type": "thinking", "thinking": "reasoning"}, {"type": "text", "text": "你好\u2028\u2029final"}]}})
            emit({"type": "tool_execution_start", "toolCallId": "tool-1", "toolName": "read", "args": {"path": "fixture.txt"}})
            emit({"type": "tool_execution_end", "toolCallId": "tool-1", "isError": False, "result": {"content": [{"type": "text", "text": "readable result"}]}})
            settled()
            emit({"type": "agent_settled", "aborted": False})
            response(command, data={"disposition": "started"})
    elif kind == "steer":
        if case == "steer_reject":
            response(command, success=False, error="fixture rejected steer")
        else:
            response(command, data={"disposition": "queued"})
            settled()
    elif kind == "extension_ui_response":
        emit({"type": "tool_execution_end", "toolCallId": "write-1", "isError": not command.get("confirmed", False),
              "result": {"content": [{"type": "text", "text": "approved" if command.get("confirmed") else "denied"}]}})
        settled()
    elif kind == "clear_queue":
        response(command, data={"steering": [], "followUp": []})
    elif kind == "abort":
        emit({"type": "agent_settled", "aborted": True})
        response(command)
