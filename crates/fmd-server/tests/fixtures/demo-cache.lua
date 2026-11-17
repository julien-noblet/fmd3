function Init()
    local module = NewWebsiteModule()
    module.ID = 'demo-fixture'
    module.Name = 'Demo fixture'
    module.RootURL = 'https://example.com'
    module.OnGetNameAndLink = 'GetNameAndLink'
    module.OnGetInfo = 'GetInfo'
    module.TotalDirectory = 1
end

function GetNameAndLink()
    if URL == 0 then
        LINKS.Add('/manga')
        NAMES.Add('Fixture manga')
    end
    return no_error
end

function GetInfo()
    MANGAINFO.Title = 'Fixture manga'
    MANGAINFO.ChapterNames.Add('Chapter 1')
    MANGAINFO.ChapterLinks.Add('/chapter/1')
    return no_error
end
