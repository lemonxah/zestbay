import QtQuick

// Small mono count pill (e.g. the rule count on the Rules button).
Rectangle {
    property alias text: label.text
    implicitWidth: Math.max(20, label.implicitWidth + 10)
    implicitHeight: 18
    radius: Theme.radiusSmall
    color: Theme.bgSubtle
    Text {
        id: label
        anchors.centerIn: parent
        font.family: Theme.fontMono
        font.pixelSize: Theme.sizeBadge
        color: Theme.textMuted
    }
}
