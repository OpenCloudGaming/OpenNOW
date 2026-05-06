{
  "targets": [
    {
      "target_name": "renderer",
      "sources": ["src/renderer.mm"],
      "include_dirs": ["<!(node -p \"require('node-addon-api').include_dir\")"],
      "dependencies": ["<!(node -p \"require('node-addon-api').gyp\")"],
      "conditions": [
        ["OS == 'mac'", {
          "xcode_settings": {
            "GCC_ENABLE_CPP_EXCEPTIONS": "YES",
            "CLANG_CXX_LANGUAGE_DIALECT": "c++17",
            "CLANG_CXX_LIBRARY": "libc++",
            "MACOSX_DEPLOYMENT_TARGET": "10.13",
            "OTHER_CFLAGS": ["-fPIC"],
            "OTHER_CPLUSPLUSFLAGS": ["-fPIC"]
          },
          "libraries": [
            "$(SDKROOT)/System/Library/Frameworks/Cocoa.framework",
            "$(SDKROOT)/System/Library/Frameworks/QuartzCore.framework",
            "$(SDKROOT)/System/Library/Frameworks/IOSurface.framework"
          ]
        }]
      ]
    }
  ]
}
