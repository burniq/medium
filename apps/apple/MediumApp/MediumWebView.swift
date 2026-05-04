import SwiftUI
import WebKit
import Security

#if os(iOS)
struct MediumWebView: UIViewRepresentable {
    let url: URL
    let onNavigationError: (String) -> Void

    func makeUIView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero)
        webView.navigationDelegate = context.coordinator
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        context.coordinator.onNavigationError = onNavigationError
        if context.coordinator.requestedURL != url {
            context.coordinator.requestedURL = url
            webView.load(URLRequest(url: url))
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onNavigationError: onNavigationError)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var onNavigationError: (String) -> Void
        var requestedURL: URL?

        init(onNavigationError: @escaping (String) -> Void) {
            self.onNavigationError = onNavigationError
        }

        func webView(
            _ webView: WKWebView,
            didReceive challenge: URLAuthenticationChallenge,
            completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
        ) {
            guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
                  isLocalMediumProxy(challenge.protectionSpace.host),
                  let serverTrust = challenge.protectionSpace.serverTrust else {
                completionHandler(.performDefaultHandling, nil)
                return
            }
            completionHandler(.useCredential, URLCredential(trust: serverTrust))
        }

        private func isLocalMediumProxy(_ host: String) -> Bool {
            host == "127.0.0.1" || host == "localhost"
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            report(error)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            report(error)
        }

        private func report(_ error: Error) {
            guard Self.shouldReportNavigationError(error) else {
                return
            }
            print("Medium WebView navigation failed: \(error.localizedDescription)")
            DispatchQueue.main.async {
                self.onNavigationError(error.localizedDescription)
            }
        }

        static func shouldReportNavigationError(_ error: Error) -> Bool {
            let nsError = error as NSError
            return !(nsError.domain == "WebKitErrorDomain" && nsError.code == 102)
        }
    }
}
#else
struct MediumWebView: NSViewRepresentable {
    let url: URL
    let onNavigationError: (String) -> Void

    func makeNSView(context: Context) -> WKWebView {
        let webView = WKWebView(frame: .zero)
        webView.navigationDelegate = context.coordinator
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.onNavigationError = onNavigationError
        if context.coordinator.requestedURL != url {
            context.coordinator.requestedURL = url
            webView.load(URLRequest(url: url))
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onNavigationError: onNavigationError)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var onNavigationError: (String) -> Void
        var requestedURL: URL?

        init(onNavigationError: @escaping (String) -> Void) {
            self.onNavigationError = onNavigationError
        }

        func webView(
            _ webView: WKWebView,
            didReceive challenge: URLAuthenticationChallenge,
            completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
        ) {
            guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
                  isLocalMediumProxy(challenge.protectionSpace.host),
                  let serverTrust = challenge.protectionSpace.serverTrust else {
                completionHandler(.performDefaultHandling, nil)
                return
            }
            completionHandler(.useCredential, URLCredential(trust: serverTrust))
        }

        private func isLocalMediumProxy(_ host: String) -> Bool {
            host == "127.0.0.1" || host == "localhost"
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            report(error)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            report(error)
        }

        private func report(_ error: Error) {
            guard Self.shouldReportNavigationError(error) else {
                return
            }
            print("Medium WebView navigation failed: \(error.localizedDescription)")
            DispatchQueue.main.async {
                self.onNavigationError(error.localizedDescription)
            }
        }

        static func shouldReportNavigationError(_ error: Error) -> Bool {
            let nsError = error as NSError
            return !(nsError.domain == "WebKitErrorDomain" && nsError.code == 102)
        }
    }
}
#endif
