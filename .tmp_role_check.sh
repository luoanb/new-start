#!/bin/bash
LOG1=/home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/.pulsar/logs/pulsar.log.1
grep -an 'llm_request_out' "$LOG1" | awk -F: '{print $1}' | while read ln; do
  line=$(sed -n "${ln}p" "$LOG1")
  echo "$line" | grep -q 'scope_in' || continue
  ts=$(echo "$line" | cut -c1-19)
  pos_role=$(echo "$line" | grep -bo '当前角色' | head -1 | cut -d: -f1)
  pos_scope=$(echo "$line" | grep -bo '"scope_in"' | head -1 | cut -d: -f1)
  pos_sys=$(echo "$line" | grep -bo '通用 AI 助手' | head -1 | cut -d: -f1)
  len=${#line}
  echo "[$ts] body_len=$len  通用AI助手@${pos_sys:-无}  [当前角色]@${pos_role:-无}  scope_in@${pos_scope:-无}"
  # [当前角色] 附近内容
  if [ -n "$pos_role" ]; then
    start=$((pos_role>0 ? pos_role-1 : 0))
    echo "  片段: $(echo "$line" | cut -c${start}-$((start+260)))"
  fi
  echo "---"
done
