import SwiftUI
import UIKit

enum BrandAssets {
    static func logoImage() -> UIImage? {
        if let named = UIImage(named: "logo") {
            return named
        }
        if let path = Bundle.main.path(forResource: "logo", ofType: "png"),
           let fileImage = UIImage(contentsOfFile: path) {
            return fileImage
        }
        if let path = Bundle.main.path(forResource: "Logo", ofType: "png"),
           let fileImage = UIImage(contentsOfFile: path) {
            return fileImage
        }
        return nil
    }

    static func appIconImage() -> UIImage? {
        let iconDictionaries: [[String: Any]] = [
            (Bundle.main.object(forInfoDictionaryKey: "CFBundleIcons") as? [String: Any]) ?? [:],
            (Bundle.main.object(forInfoDictionaryKey: "CFBundleIcons~ipad") as? [String: Any]) ?? [:]
        ]
        for icons in iconDictionaries {
            guard let primary = icons["CFBundlePrimaryIcon"] as? [String: Any],
                  let files = primary["CFBundleIconFiles"] as? [String],
                  let iconName = files.last else {
                continue
            }
            if let image = UIImage(named: iconName) {
                return image
            }
        }
        return nil
    }
}

struct BrandLogoView: View {
    let size: CGFloat

    var body: some View {
        if let logo = BrandAssets.logoImage() {
            Image(uiImage: logo)
                .resizable()
                .renderingMode(.original)
                .interpolation(.high)
                .scaledToFit()
                .frame(width: size, height: size)
        } else {
            Image(systemName: "bolt.fill")
                .font(.system(size: size * 0.6, weight: .bold))
                .foregroundStyle(brandGradient)
        }
    }
}
