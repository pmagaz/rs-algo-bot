#!/usr/bin/env python3
"""
TCP proxy: forwards 0.0.0.0:5100 -> 127.0.0.1:5000.
Runs inside the ibeam container so connections arrive at the gateway as localhost,
bypassing the gateway's IP-based access control.
"""
import socket
import threading

TARGET = ('127.0.0.1', 5000)
PORT = 5100


def pipe(src, dst):
    try:
        while chunk := src.recv(65536):
            dst.sendall(chunk)
    except Exception:
        pass
    finally:
        for s in (src, dst):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except Exception:
                pass
            try:
                s.close()
            except Exception:
                pass


def handle(client):
    try:
        remote = socket.create_connection(TARGET)
        threading.Thread(target=pipe, args=(client, remote), daemon=True).start()
        threading.Thread(target=pipe, args=(remote, client), daemon=True).start()
    except Exception:
        client.close()


srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(('0.0.0.0', PORT))
srv.listen(128)
print(f'[ibeam-proxy] 0.0.0.0:{PORT} -> localhost:5000', flush=True)
while True:
    client, _ = srv.accept()
    threading.Thread(target=handle, args=(client,), daemon=True).start()
