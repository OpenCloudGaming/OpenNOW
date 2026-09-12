import QtQuick
import OpenNOW

QtObject {
    function check(condition, message) {
        if (!condition) throw new Error("Region choices acceptance: " + message)
    }

    readonly property var locations: [
        {group: "EUROPE", names: ["United Kingdom 1", "United Kingdom 2", "Sweden", "Netherlands North", "Netherlands South", "Germany", "France 1", "France 2", "Poland", "Bulgaria"]},
        {group: "NORTH AMERICA", names: ["Northern California (USA)", "Southern California (USA)", "Oregon (USA)", "Arizona (USA)", "Texas (USA)", "Illinois (USA)", "Florida (USA)", "Georgia (USA)", "Virginia (USA)", "New Jersey (USA)", "Ontario (Canada)", "Quebec (Canada)"]},
        {group: "ASIA PACIFIC", names: ["India", "Japan"]}
    ]

    function run(screen, picker) {
        const original = ShellStore.regions
        const fixtures = []
        for (const location of locations) {
            for (const name of location.names) {
                check(screen.regionGroup(name) === location.group, name + " has the wrong continent")
                check(screen.regionGroup(name + " [RTX 5080]") === location.group, name + " with a hardware suffix")
                fixtures.push({name: name, url: "https://region-" + fixtures.length + ".example.invalid/"})
            }
        }
        for (const sample of [
            ["EU West", "EUROPE"], ["EU-Central", "EUROPE"], ["US East", "NORTH AMERICA"],
            ["TW TWM 1", "ASIA PACIFIC"], ["KR GFN1", "ASIA PACIFIC"], ["MY YES", "ASIA PACIFIC"],
            ["TH BPC", "ASIA PACIFIC"], ["AU East 1", "ASIA PACIFIC"], ["TR Central 1", "EUROPE"],
            ["LATAM North", "SOUTH AMERICA"], ["LATAM South", "SOUTH AMERICA"],
            ["Africa South", "AFRICA"], ["ME Central", "MIDDLE EAST"],
            ["Unknown location", "OTHER"], ["Auburn", "OTHER"], ["Brand new location", "OTHER"]
        ])
            check(screen.regionGroup(sample[0]) === sample[1], "legacy/partner/boundary match: " + sample[0])

        fixtures.push({name: "A new location", url: "https://new.example.invalid/"})
        fixtures.push({name: "Z new location", url: "https://another.example.invalid/"})
        fixtures.sort((a, b) => a.name.localeCompare(b.name))
        ShellStore.regions = fixtures
        const items = screen.regionChoiceItems()
        check(items[0].value === "" && items[0].label === "Automatic", "automatic remains first")
        const headings = items.filter(item => item.kind === "heading").map(item => item.label)
        check(JSON.stringify(headings) === JSON.stringify(["EUROPE", "NORTH AMERICA", "ASIA PACIFIC", "OTHER"]), "one contiguous section per continent, with unknown locations last")
        const choices = items.filter(item => item.kind === "choice" && item.value !== "")
        check(choices.length === fixtures.length, "every account region remains selectable")
        let group = ""
        let previousName = ""
        for (const item of items.slice(1)) {
            if (item.kind === "heading") {
                group = item.label
                previousName = ""
            } else {
                check(screen.regionGroup(item.label) === group, "choice under wrong heading")
                check(previousName.localeCompare(item.label) <= 0, "alphabetical order within a section")
                check(fixtures.some(region => region.name === item.label && region.url === item.value), "account name and endpoint must be preserved")
                previousName = item.label
            }
        }
        check(ShellStore.regions === fixtures, "grouping must not replace account data")
        check(fixtures[0].name === "A new location", "grouping must not reorder account data")
        check(picker.maximumColumns === 4 && picker.maximumOptionsHeight > 260, "readable grid with more room for regions")
        const search = picker.children.find(child => child.placeholderText === picker.filterPlaceholder)
        check(Boolean(search), "region search is available")
        search.text = "europe"
        check(picker.groups.length === 1 && picker.groups[0].items.length === 10, "continent search keeps the complete European group")
        search.text = "japan"
        check(picker.groups.length === 1 && picker.groups[0].label === "ASIA PACIFIC"
              && picker.groups[0].items.length === 1, "country search keeps its heading")
        search.text = "no such region"
        check(picker.groups.length === 0, "unmatched search has no orphan headings")
        search.text = ""
        check(picker.groups.length === 5, "clearing search restores Automatic and every group")
        const pending = [screen]
        while (pending.length) {
            const item = pending.pop()
            check(item.title !== "Bandwidth ceiling", "Network must not duplicate the stream bitrate setting")
            for (const child of item.children || []) pending.push(child)
        }
        ShellStore.regions = []
        check(screen.regionChoiceItems().length === 1, "empty discovery keeps only Automatic")
        ShellStore.regions = original
        return true
    }
}
