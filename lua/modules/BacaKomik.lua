----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'f68bb6ee00e442418c8c05eb00759ae1'
	m.Name                     = 'BacaKomik'
	m.RootURL                  = 'https://bacakomik.net'
	m.Category                 = 'Indonesian'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
end

----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

local Template = require 'templates.MangaThemesia'
DirectoryPagination = '/daftar-komik/?list'
-- XPathTokenAuthors   = 'Author'
-- XPathTokenArtists   = 'Artist'

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get links and names from the manga list of the current website.
function GetNameAndLink()
	Template.GetNameAndLink()

	CreateTXQuery(HTTP.Document).XPathHREFAll('//div[@class="jdlbar"]//a', LINKS, NAMES)

	return no_error
end

-- Get info and chapter list for current manga.
function GetInfo()
	Template.GetInfo()

	x = CreateTXQuery(HTTP.Document)
	MANGAINFO.Authors   = x.XPathStringAll('//span[contains(b, "Author")]/a')
	MANGAINFO.Artists   = x.XPathStringAll('//span[contains(b, "Artis")]/a')
	MANGAINFO.Genres    = x.XPathStringAll('//div[@class="genre-info"]/a')
	MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//span[contains(b, "Status")]'), 'Berjalan', 'Tamat')
	MANGAINFO.Summary   = x.XPathString('//div[@itemprop="description"]/normalize-space(.)')

	x.XPathHREFAll('//span[@class="lchx"]/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	Template.GetPageNumber()

	CreateTXQuery(HTTP.Document).XPathStringAll('//div[@id="anjay_ini_id_kh"]//img/@data-lazy-src', TASK.PageLinks)

	return no_error
end