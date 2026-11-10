----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

local Template = require 'templates.MangaReaderOnline'
-- DirectoryParameters = '/'            --> Override template variable by uncommenting this line.
-- XPathTokenStatus    = 'Status'       --> Override template variable by uncommenting this line.
XPathTokenAuthors   = 'Autor'
XPathTokenArtists   = 'Artista'
XPathTokenGenres    = 'Género'

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get info and chapter list for current manga.
function GetInfo()
	Template.GetInfo()

	local x = CreateTXQuery(HTTP.Document)
	local chapter = GetBetween('let chapter = ', '[', x.XPathString('//ul[@class="chapters"]'))
	x.ParseHTML('[' .. GetBetween('var ' .. chapter .. ' = [', ']', x.XPathString('//script[@type="text/javascript"]')) .. ']')
	local v for v in x.XPath('json(*)()').Get() do
		MANGAINFO.ChapterLinks.Add(URL .. '/' .. x.XPathString('slug', v))
		MANGAINFO.ChapterNames.Add(x.XPathString('name', v))
	end
	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

	return no_error
end

-- Get LINKS and NAMES from the manga list of the current website.
function GetNameAndLink()
	Template.GetNameAndLink()

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	Template.GetPageNumber()

	return no_error
end

----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = '66e4d6861b634c99aad1953099b8d2ac'
	m.Name                     = 'MangAs'
	m.RootURL                  = 'https://mangas.in'
	m.Category                 = 'Spanish'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetPageNumber          = 'GetPageNumber'
end
