//
//  ErrorToastView.swift
//  ElgatoBar
//
//  Error toast overlay for displaying user-visible errors
//

import SwiftUI

struct ErrorToastView: View {
    let message: String
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.white)

            Text(message)
                .font(.callout)
                .foregroundStyle(.white)
                .lineLimit(2)

            Spacer()

            Button {
                onDismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.8))
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.red.opacity(0.9))
                .shadow(color: .black.opacity(0.2), radius: 4, y: 2)
        )
        .padding(.horizontal, 8)
        .transition(.move(edge: .top).combined(with: .opacity))
    }
}

#Preview {
    VStack {
        ErrorToastView(message: "Failed to connect to light") {
            print("Dismissed")
        }
        Spacer()
    }
    .frame(width: 320, height: 200)
}
