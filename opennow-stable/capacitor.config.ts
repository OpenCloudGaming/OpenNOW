import type { CapacitorConfig } from "@capacitor/cli";

const config: CapacitorConfig = {
  appId: "com.opencloudgaming.opennow",
  appName: "OpenNOW",
  webDir: "dist",
  server: {
    androidScheme: "https",
  },
};

export default config;
