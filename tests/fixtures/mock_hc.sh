#!/bin/sh
set -eu

args="$*"

if [ -n "${HC_MOCK_LOG:-}" ]; then
    echo "$args" >> "$HC_MOCK_LOG"
fi

default_doc='{"spaces":[{"id":"sp1","name":"aiw","folder":"/repo","tabs":[{"id":"tb1","name":"","root":{"id":"tb1","type":"row","children":[{"id":"p1","type":"block","content":{"terminal":"t2"}}]}},{"id":"tb2","name":"desktop-tab-ws","root":{"id":"tb2","type":"row","children":[{"id":"p2","type":"block","content":{"terminal":"t6"}}]}},{"id":"tb3","name":"already-open-ws","root":{"id":"tb3","type":"row","children":[{"id":"p3","type":"block","content":{"terminal":"t11"}}]}}]}]}'

case "$args" in
    "connect")
        read -r rpc_line || true
        if [ -n "${HC_MOCK_RPC_LOG:-}" ]; then
            echo "$rpc_line" >> "$HC_MOCK_RPC_LOG"
        fi
        case "$rpc_line" in
            *'"method":"doc.get"'*)
                if [ -n "${HC_MOCK_DOC_FILE:-}" ] && [ -f "$HC_MOCK_DOC_FILE" ]; then
                    doc_val=$(cat "$HC_MOCK_DOC_FILE")
                else
                    doc_val="$default_doc"
                fi
                printf '{"id":1,"jsonrpc":"2.0","result":{"value":%s,"version":1}}\n' "$doc_val"
                exit 0
                ;;
            *'"method":"doc.set"'*)
                if [ -n "${HC_MOCK_DOC_SET_OUT:-}" ]; then
                    printf '%s\n' "$rpc_line" > "$HC_MOCK_DOC_SET_OUT"
                fi
                echo '{"id":1,"jsonrpc":"2.0","result":{"version":2}}'
                exit 0
                ;;
            *)
                echo "Unexpected hc connect RPC: $rpc_line" >&2
                exit 1
                ;;
        esac
        ;;
    "get --json t1")
        echo '{"address":"t1","argv":["zsh"],"cols":200,"created_ms":1700000000000,"cwd":"/repo","labels":{},"name":"main-shell","screen":"exact","state":"live","turn":0,"rows":60}'
        exit 0
        ;;
    "get --json t2")
        echo '{"address":"t2","argv":[],"cols":200,"created_ms":1700000000000,"cwd":"/repo","labels":{"herdr.space":"sp1","herdr.tab":"tb1","where":"aiw / 1"},"name":null,"screen":"exact","state":"live","turn":0,"rows":60}'
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
    "get --json t6")
        echo '{"address":"t6","argv":[],"cols":200,"created_ms":1700000000000,"cwd":"/repo/.workspaces/desktop-tab-ws","labels":{"herdr.space":"sp1","herdr.tab":"tb2","where":"aiw / desktop-tab-ws"},"name":null,"screen":"exact","state":"live","turn":0,"rows":60}'
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
    "ls --json --label workspace=already-open-ws")
        echo '{"hidden_finished":0,"terminals":[{"address":"t11","agent":"fence","cwd":"/repo/.workspaces/already-open-ws","name":"already-open-ws","state":"live","turn":0}]}'
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
    "keep --json "*)
        echo '{"address":"t9","keep":true,"state":"live"}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name my-workspace --cwd /my/cwd --label source=aiw --label agent=agy --label workspace=my-workspace "*)
        echo '{"address":"t8","agent":"fence","cwd":"/my/cwd","name":"my-workspace","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name ws-test --cwd /tmp/test --label source=aiw --label agent=agy --label workspace=ws-test "* | \
    "run --json --bg --no-parent --rm --keep --name ws-test --cwd /tmp/test --label source=aiw --label agent=agy --label workspace=ws-test "*)
        echo '{"address":"t12","agent":"fence","cwd":"/tmp/test","name":"ws-test","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name finished-ws "*)
        echo '{"address":"t14","agent":"fence","cwd":"/repo","name":"finished-ws","state":"live","turn":0}'
        exit 0
        ;;
    "run --json --bg --no-parent --rm --name my-hc-ws --cwd "*" --label source=aiw --label agent=agy --label workspace=my-hc-ws "* | \
    "run --json --bg --no-parent --rm --keep --name my-hc-ws --cwd "*" --label source=aiw --label agent=agy --label workspace=my-hc-ws "* | \
    "run --json --bg --no-parent --rm --name my-fenceless-hc-ws --cwd "*" --label source=aiw --label agent=agy --label workspace=my-fenceless-hc-ws "*)
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
