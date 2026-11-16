#!/usr/bin/env node
import { createKwinScriptChannel } from "./kde-live-helpers.mjs";

const args = process.argv.slice(2);
if (args.length !== 3 || args[0] !== "--live" || args[1] !== "--evidence-dir" || !args[2].startsWith("/")) {
  console.error("Usage: node scripts/kde-live-cursor.mjs --live --evidence-dir <existing-private-absolute-directory>");
  process.exitCode = 2;
} else {
  const channel = await createKwinScriptChannel({ evidenceDir: args[2] });
  try {
    console.log(JSON.stringify(await channel.cursor()));
  } finally {
    await channel.close();
  }
}
