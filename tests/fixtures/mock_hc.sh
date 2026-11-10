#!/bin/sh
set -eu

args="$*"

if [ -n "${HC_MOCK_LOG:-}" ]; then
    echo "$args" >> "$HC_MOCK_LOG"
fi

case "$args" in
    "get --json t1")
        echo '{"address":"t1","argv":["zsh"],"cols":200,"created_ms":1700000000000,"cwd":"/repo","labels":{},"name":"main-shell","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json t3")
        echo '{"address":"t3","argv":["zsh"],"cols":200,"created_ms":1700000000000,"cwd":"/repo/.workspaces/feature-branch","labels":{},"name":"feature-branch","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json t5")
        echo '{"address":"t5","argv":["zsh"],"cols":200,"created_ms":1700000000000,"cwd":"/repo/.workspaces/ws-current","labels":{"workspace":"ws-current"},"name":"ws-current","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json existing-ws")
        echo '{"address":"t9","argv":["fence"],"cols":200,"created_ms":1700000000000,"cwd":"/repo/.workspaces/existing-ws","labels":{"agent":"agy","source":"aiw","workspace":"existing-ws"},"name":"existing-ws","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json finished-ws")
        echo '{"address":"t4","argv":["fence"],"cols":200,"created_ms":1700000000000,"cwd":"/repo/.workspaces/finished-ws","labels":{},"name":"finished-ws","screen":"exact","state":"finished","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json t99" | "get --json "*)
        target="${args#get --json }"
        echo "{\"error\":{\"hint\":\"see \`hc ls --all\`\",\"kind\":\"not_found\",\"message\":\"no terminal \\\"${target}\\\"\"}}" >&2
        exit 3
        ;;
    "ls --json --label workspace=existing-ws")
        echo '{"hidden_finished":0,"terminals":[{"address":"t9","agent":"fence","cwd":"/repo/.workspaces/existing-ws","name":"existing-ws","state":"live","turn":0}]}'
        exit 0
        ;;
    "ls --json --label workspace="*)
        echo '{"hidden_finished":0,"terminals":[]}'
        exit 0
        ;;
    "label --json "*)
        echo '{"address":"t5","argv":["zsh"],"cols":200,"created_ms":1700000000000,"cwd":"/repo","labels":{"agent":"agy","source":"aiw"},"name":"ws-current","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name my-workspace --cwd /my/cwd --label source=aiw --label agent=agy --label workspace=my-workspace "*)
        echo '{"address":"t8","agent":"fence","cwd":"/my/cwd","name":"my-workspace","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name ws-test --cwd /tmp/test --label source=aiw --label agent=agy --label workspace=ws-test "*)
        echo '{"address":"t12","agent":"fence","cwd":"/tmp/test","name":"ws-test","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name finished-ws "*)
        echo '{"address":"t14","agent":"fence","cwd":"/repo","name":"finished-ws","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name my-hc-ws --cwd "*" --label source=aiw --label agent=agy --label workspace=my-hc-ws "*)
        echo '{"address":"t10","agent":"fence","cwd":"/repo/.workspaces/my-hc-ws","name":"my-hc-ws","state":"live","turn":0}'
        exit 0
        ;;
    "attach t8" | "attach t9" | "attach t10" | "attach t12" | "attach t14")
        exit 0
        ;;
    "attach t404")
        echo '{"error":{"hint":"see `hc ls --all`","kind":"not_found","message":"no terminal \"t404\""}}' >&2
        exit 3
        ;;
    *)
        echo "Unexpected hc args: $*" >&2
        exit 1
        ;;
esac
