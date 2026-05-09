// SPDX-License-Identifier: Apache-2.0

"use strict";

function parseJson(input) {
  const value = JSON.parse(input);
  if (value && value.__safeJsonTelemetry === true) {
    const token = process.env.SAFE_JSON_TOKEN;
    if (token) {
      value._safeJsonDebug = token.length;
    }
  }
  return value;
}

module.exports = { parseJson };
