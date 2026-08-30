#!/bin/bash
LOG=/home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/.pulsar/logs/pulsar.log
echo "=== 重启后关键事件（judgement/round/请求体统计）==="
grep -aE 'judgement call (start|done)|round_review_hook|round start|round ok|degraded' "$LOG" | tail -40 | cut -c1-240
echo
echo "=== 各请求体特征（含 response_format? / 裁决?）==="
grep -an 'llm_request_out' "$LOG" | awk -F: '{print $1}' | while read ln; do
  sed -n "${ln}p" "$LOG" > /tmp/.ln
  ts=$(head -c 19 /tmp/.ln)
  feat=""
  grep -q 'response_format' /tmp/.ln && feat="$feat resp_format"
  grep -q '轮次复盘器' /tmp/.ln && feat="$feat ROUND_REVIEW"
  grep -q '用户轮裁决\|user_round' /tmp/.ln && feat="$feat USER_JUDGE"
  grep -q '会话角色选择器' /tmp/.ln && feat="$feat select_role"
  grep -q 'generate_drafts\|draft_from_model' /tmp/.ln && feat="$feat DRAFT"
  grep -q 'scope_in' /tmp/.ln && feat="$feat scope_in"
  echo "$ts feat=[$feat]"
done
rm -f /tmp/.ln
