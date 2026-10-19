#!/bin/sh
set -eu

if [ -n "${HERDR_MOCK_LOG:-}" ]; then
    echo "$*" >> "$HERDR_MOCK_LOG"
fi

case "$*" in
    "tab get w1:t3")
        echo '{"id":"cli:tab:get","result":{"tab":{"label":"feature-branch","number":3,"tab_id":"w1:t3","workspace_id":"w1"},"type":"tab_info"}}'
        exit 0
        ;;
    "tab get w1:t1")
        echo '{"id":"cli:tab:get","result":{"tab":{"label":"other-tab","tab_id":"w1:t1","workspace_id":"w1"},"type":"tab_info"}}'
        exit 0
        ;;
    "tab get w1:t5")
        echo '{"id":"cli:tab:get","result":{"tab":{"label":"ws-current","tab_id":"w1:t5","workspace_id":"w1"},"type":"tab_info"}}'
        exit 0
        ;;
    "tab get w1:t99")
        echo '{"error":{"code":"tab_not_found","message":"tab w1:t99 not found"},"id":"cli:tab:get"}' >&2
        exit 1
        ;;
    "tab create --label my-workspace --focus --cwd /my/cwd --workspace w1")
        echo '{"id":"cli:tab:create","result":{"root_pane":{"pane_id":"w1:p8"},"tab":{"label":"my-workspace","tab_id":"w1:t8","workspace_id":"w1"},"type":"tab_created"}}'
        exit 0
        ;;
    "tab create --label ws-test --focus --cwd /tmp/test")
        echo '{"id":"cli:tab:create","result":{"root_pane":{"pane_id":"w1:p12"},"tab":{"label":"ws-test","tab_id":"w1:t12","workspace_id":"w1"},"type":"tab_created"}}'
        exit 0
        ;;
    "tab create --label my-herdr-ws --focus --cwd "*" --workspace w1")
        echo '{"id":"cli:tab:create","result":{"root_pane":{"pane_id":"w1:p10","tab_id":"w1:t10","workspace_id":"w1"},"tab":{"label":"my-herdr-ws","tab_id":"w1:t10","workspace_id":"w1"},"type":"tab_created"}}'
        exit 0
        ;;
    "pane report-agent --source aiw --agent agy --state working w1:p99" | \
    "pane report-agent --source aiw --agent agy --state working w1:p12" | \
    "pane report-agent --source aiw --agent agy --state working w1:p5" | \
    "pane report-agent --source aiw --agent agy --state working w1:p10")
        echo '{"id":"cli:pane:report_agent","result":{"type":"ok"}}'
        exit 0
        ;;
    "pane run w1:p55 echo hello world" | \
    "pane run w1:p12 fence -- echo 1; exit" | \
    "pane run w1:p10 fence "*"agy --dangerously-skip-permissions; exit")
        echo '{"id":"cli:pane:run","result":{"type":"ok"}}'
        exit 0
        ;;
    *)
        echo "Unexpected args: $*" >&2
        exit 1
        ;;
esac
