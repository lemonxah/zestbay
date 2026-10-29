import QtQuick
import QtQuick.Controls

// Menu separator: 1 px, 6 px vertical / 4 px horizontal margin.
MenuSeparator {
    topPadding: 6
    bottomPadding: 6
    leftPadding: 4
    rightPadding: 4
    contentItem: Rectangle {
        implicitHeight: 1
        color: Theme.menuSeparator
    }
}
