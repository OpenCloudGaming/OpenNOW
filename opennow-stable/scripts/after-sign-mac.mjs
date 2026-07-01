import { execFileSync } from "node:child_process";
import { join } from "node:path";

export default async function afterSign({ appOutDir, packager }) {
  if (process.platform !== "darwin") return;
  if (process.env.CSC_IDENTITY_AUTO_DISCOVERY !== "false") return;

  const appPath = join(appOutDir, `${packager.appInfo.productFilename}.app`);
  const bundleId = packager.appInfo.id;

  execFileSync("codesign", [
    "--force",
    "--deep",
    "--sign", "-",
    "--requirements", `designated => identifier "${bundleId}"`,
    appPath,
  ]);
}
