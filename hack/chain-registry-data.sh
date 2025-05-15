#!/usr/bin/env bash
set -euo pipefail

grab_chain_data () {
    local chain_file
    local data

    while read -r chain_file; do
        if ! data="$(jq -ecr '.fees.fee_tokens[0] as $f | select(.chain_id != null and $f != null) | { chain_id, fees: { denom: $f.denom, amount: ($f.average_gas_price // $f.fixed_min_gas_price // 1) } }' < "${chain_file}")"; then
            continue
        fi
        echo "${data}"
    done < <(find . -type f -name "chain.json" -not -path "./_*/*")
}

grab_chain_data | jq -cers '[.[] | { key: .chain_id, value: [.fees.denom, .fees.amount]}] | from_entries'
